-- PostgreSQL initialization script for nTimes Naver News Crawler
-- Copyright (c) 2024 hephaex@gmail.com
-- License: GPL v3

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";  -- Trigram matching for fuzzy search
CREATE EXTENSION IF NOT EXISTS "btree_gin"; -- GIN indexes on B-tree types

-- Set timezone to UTC for consistency
SET timezone = 'UTC';

-- Create enum types for better type safety
CREATE TYPE article_category AS ENUM (
    'politics',
    'economy',
    'society',
    'culture',
    'world',
    'it',
    'sports',
    'entertainment',
    'unknown'
);

CREATE TYPE processing_status AS ENUM (
    'pending',
    'processing',
    'completed',
    'failed'
);

-- ============================================================================
-- Articles Raw Table - Primary storage for crawled articles
-- ============================================================================
CREATE TABLE IF NOT EXISTS articles_raw (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    -- Naver News identifiers
    oid VARCHAR(10) NOT NULL,           -- Publisher/Organization ID
    aid VARCHAR(20) NOT NULL,           -- Article ID

    -- Content fields
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    content_hash VARCHAR(64),           -- SHA256 hash for deduplication

    -- Metadata
    url TEXT NOT NULL,
    category article_category NOT NULL DEFAULT 'unknown',
    publisher VARCHAR(255),
    author VARCHAR(255),

    -- Timestamps
    published_at TIMESTAMPTZ NOT NULL,
    crawled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Processing metadata
    comment_count INTEGER DEFAULT 0,
    view_count BIGINT,
    like_count INTEGER,

    -- Full-text search vector
    search_vector tsvector,

    -- Audit fields
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Unique constraint for deduplication
    CONSTRAINT unique_article UNIQUE (oid, aid)
);

-- Indexes for articles_raw
CREATE INDEX idx_articles_category ON articles_raw(category);
CREATE INDEX idx_articles_publisher ON articles_raw(publisher);
CREATE INDEX idx_articles_published_at ON articles_raw(published_at DESC);
CREATE INDEX idx_articles_crawled_at ON articles_raw(crawled_at DESC);
CREATE INDEX idx_articles_content_hash ON articles_raw(content_hash) WHERE content_hash IS NOT NULL;

-- GIN index for full-text search
CREATE INDEX idx_articles_search_vector ON articles_raw USING GIN(search_vector);

-- Composite index for common query patterns
CREATE INDEX idx_articles_category_published ON articles_raw(category, published_at DESC);
CREATE INDEX idx_articles_oid_aid ON articles_raw(oid, aid);

-- Trigger to automatically update search_vector
CREATE OR REPLACE FUNCTION articles_search_vector_update() RETURNS trigger AS $$
BEGIN
    NEW.search_vector :=
        setweight(to_tsvector('simple', COALESCE(NEW.title, '')), 'A') ||
        setweight(to_tsvector('simple', COALESCE(NEW.content, '')), 'B') ||
        setweight(to_tsvector('simple', COALESCE(NEW.publisher, '')), 'C');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trig_articles_search_vector_update
    BEFORE INSERT OR UPDATE ON articles_raw
    FOR EACH ROW EXECUTE FUNCTION articles_search_vector_update();

-- Trigger to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column() RETURNS trigger AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trig_articles_updated_at
    BEFORE UPDATE ON articles_raw
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Comments Raw Table - Storage for article comments and replies
-- ============================================================================
CREATE TABLE IF NOT EXISTS comments_raw (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    -- Comment identifiers
    comment_no BIGINT NOT NULL,         -- Naver comment number
    ticket BIGINT,                      -- Comment ticket ID

    -- Article relationship
    article_id UUID NOT NULL REFERENCES articles_raw(id) ON DELETE CASCADE,
    oid VARCHAR(10) NOT NULL,           -- Article oid for quick lookup
    aid VARCHAR(20) NOT NULL,           -- Article aid for quick lookup

    -- Comment hierarchy (for nested replies)
    parent_comment_no BIGINT,           -- NULL for top-level comments
    depth INTEGER NOT NULL DEFAULT 0,   -- 0 for top-level, 1+ for replies

    -- Content
    content TEXT NOT NULL,

    -- User information (may be anonymized/deleted by Naver)
    user_id_no VARCHAR(50),
    user_name VARCHAR(255),
    is_deleted BOOLEAN DEFAULT FALSE,

    -- Metadata
    like_count INTEGER DEFAULT 0,
    dislike_count INTEGER DEFAULT 0,
    reply_count INTEGER DEFAULT 0,

    -- Timestamps
    written_at TIMESTAMPTZ NOT NULL,
    modified_at TIMESTAMPTZ,
    crawled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Unique constraint for deduplication
    CONSTRAINT unique_comment UNIQUE (comment_no, article_id)
);

-- Indexes for comments_raw
CREATE INDEX idx_comments_article_id ON comments_raw(article_id);
CREATE INDEX idx_comments_comment_no ON comments_raw(comment_no);
CREATE INDEX idx_comments_parent ON comments_raw(parent_comment_no) WHERE parent_comment_no IS NOT NULL;
CREATE INDEX idx_comments_written_at ON comments_raw(written_at DESC);
CREATE INDEX idx_comments_oid_aid ON comments_raw(oid, aid);

-- Composite index for hierarchical queries
CREATE INDEX idx_comments_article_parent_depth ON comments_raw(article_id, parent_comment_no, depth);

-- Trigger to update updated_at
CREATE TRIGGER trig_comments_updated_at
    BEFORE UPDATE ON comments_raw
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Ontology Triples Table - Knowledge graph storage (RDF-like)
-- ============================================================================
CREATE TABLE IF NOT EXISTS ontology_triples (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    -- Article source
    article_id UUID NOT NULL REFERENCES articles_raw(id) ON DELETE CASCADE,

    -- RDF Triple: Subject - Predicate - Object
    subject VARCHAR(500) NOT NULL,      -- Entity or concept
    predicate VARCHAR(255) NOT NULL,    -- Relationship type
    object VARCHAR(500) NOT NULL,       -- Related entity or value

    -- Metadata
    confidence_score REAL CHECK (confidence_score >= 0 AND confidence_score <= 1),
    extraction_method VARCHAR(50),      -- 'llm', 'rule-based', 'manual'
    sentence_context TEXT,              -- Original sentence for verification

    -- Timestamps
    extracted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    verified_at TIMESTAMPTZ,

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for ontology_triples
CREATE INDEX idx_ontology_article_id ON ontology_triples(article_id);
CREATE INDEX idx_ontology_subject ON ontology_triples(subject);
CREATE INDEX idx_ontology_predicate ON ontology_triples(predicate);
CREATE INDEX idx_ontology_object ON ontology_triples(object);
CREATE INDEX idx_ontology_confidence ON ontology_triples(confidence_score DESC);

-- Composite index for triple queries
CREATE INDEX idx_ontology_spo ON ontology_triples(subject, predicate, object);
CREATE INDEX idx_ontology_sp ON ontology_triples(subject, predicate);

-- GIN index for pattern matching
CREATE INDEX idx_ontology_subject_trgm ON ontology_triples USING GIN(subject gin_trgm_ops);
CREATE INDEX idx_ontology_object_trgm ON ontology_triples USING GIN(object gin_trgm_ops);

-- Trigger to update updated_at
CREATE TRIGGER trig_ontology_updated_at
    BEFORE UPDATE ON ontology_triples
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Crawl Jobs Table - Job tracking and distributed crawling coordination
-- ============================================================================
CREATE TABLE IF NOT EXISTS crawl_jobs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    -- Job metadata
    job_type VARCHAR(50) NOT NULL,      -- 'article', 'comment', 'category'
    category article_category,
    status processing_status NOT NULL DEFAULT 'pending',

    -- Target information
    target_url TEXT,
    target_count INTEGER,               -- Max articles/comments to crawl

    -- Progress tracking
    items_total INTEGER DEFAULT 0,
    items_processed INTEGER DEFAULT 0,
    items_failed INTEGER DEFAULT 0,

    -- Worker information
    worker_id VARCHAR(255),             -- Hostname or worker identifier
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    last_heartbeat TIMESTAMPTZ,

    -- Error tracking
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for crawl_jobs
CREATE INDEX idx_crawl_jobs_status ON crawl_jobs(status);
CREATE INDEX idx_crawl_jobs_category ON crawl_jobs(category);
CREATE INDEX idx_crawl_jobs_worker ON crawl_jobs(worker_id) WHERE worker_id IS NOT NULL;
CREATE INDEX idx_crawl_jobs_created ON crawl_jobs(created_at DESC);

-- Trigger to update updated_at
CREATE TRIGGER trig_crawl_jobs_updated_at
    BEFORE UPDATE ON crawl_jobs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- Embedding Metadata Table - Track vector embeddings in OpenSearch
-- ============================================================================
CREATE TABLE IF NOT EXISTS embedding_metadata (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    -- Source reference
    article_id UUID NOT NULL REFERENCES articles_raw(id) ON DELETE CASCADE,

    -- OpenSearch metadata
    opensearch_index VARCHAR(255) NOT NULL,
    opensearch_doc_id VARCHAR(255) NOT NULL,

    -- Embedding details
    model_name VARCHAR(255) NOT NULL,   -- e.g., 'multilingual-e5-large'
    embedding_dimension INTEGER NOT NULL,
    chunk_index INTEGER DEFAULT 0,      -- For articles split into chunks
    chunk_text TEXT,

    -- Status
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT unique_embedding UNIQUE (article_id, chunk_index, model_name)
);

-- Indexes for embedding_metadata
CREATE INDEX idx_embedding_article_id ON embedding_metadata(article_id);
CREATE INDEX idx_embedding_opensearch_doc ON embedding_metadata(opensearch_index, opensearch_doc_id);
CREATE INDEX idx_embedding_model ON embedding_metadata(model_name);

-- ============================================================================
-- Statistics Views - Pre-computed aggregations for monitoring
-- ============================================================================

-- Daily article statistics by category
CREATE OR REPLACE VIEW daily_article_stats AS
SELECT
    DATE(published_at) AS date,
    category,
    COUNT(*) AS article_count,
    COUNT(DISTINCT publisher) AS publisher_count,
    AVG(comment_count) AS avg_comments,
    SUM(view_count) AS total_views
FROM articles_raw
WHERE published_at >= CURRENT_DATE - INTERVAL '30 days'
GROUP BY DATE(published_at), category
ORDER BY date DESC, category;

-- Crawling progress view
CREATE OR REPLACE VIEW crawl_progress AS
SELECT
    id,
    job_type,
    category,
    status,
    CASE
        WHEN items_total > 0 THEN ROUND((items_processed::NUMERIC / items_total) * 100, 2)
        ELSE 0
    END AS progress_percent,
    items_processed,
    items_total,
    items_failed,
    worker_id,
    created_at,
    completed_at,
    EXTRACT(EPOCH FROM (COALESCE(completed_at, NOW()) - started_at)) AS duration_seconds
FROM crawl_jobs
ORDER BY created_at DESC;

-- Top publishers by article count
CREATE OR REPLACE VIEW top_publishers AS
SELECT
    publisher,
    COUNT(*) AS article_count,
    COUNT(DISTINCT category) AS category_count,
    MIN(published_at) AS first_article,
    MAX(published_at) AS latest_article
FROM articles_raw
WHERE publisher IS NOT NULL
GROUP BY publisher
ORDER BY article_count DESC;

-- ============================================================================
-- Utility Functions
-- ============================================================================

-- Function to get article with comments
CREATE OR REPLACE FUNCTION get_article_with_comments(p_article_id UUID)
RETURNS TABLE (
    article_id UUID,
    title TEXT,
    content TEXT,
    comment_no BIGINT,
    comment_content TEXT,
    comment_depth INTEGER
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        a.id,
        a.title,
        a.content,
        c.comment_no,
        c.content,
        c.depth
    FROM articles_raw a
    LEFT JOIN comments_raw c ON a.id = c.article_id
    WHERE a.id = p_article_id
    ORDER BY c.written_at DESC;
END;
$$ LANGUAGE plpgsql;

-- Function to calculate content similarity (simple trigram-based)
CREATE OR REPLACE FUNCTION calculate_content_similarity(p_text1 TEXT, p_text2 TEXT)
RETURNS REAL AS $$
BEGIN
    RETURN similarity(p_text1, p_text2);
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Initial Data and Configuration
-- ============================================================================

-- Insert default crawl job for initial setup verification
INSERT INTO crawl_jobs (job_type, status, target_count, items_total)
VALUES ('article', 'completed', 0, 0)
ON CONFLICT DO NOTHING;

-- Grant permissions (adjust as needed for your security requirements)
-- GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA public TO ntimes;
-- GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO ntimes;
-- GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO ntimes;

-- ============================================================================
-- Performance Tuning Settings
-- ============================================================================

-- Analyze tables for query planner
ANALYZE articles_raw;
ANALYZE comments_raw;
ANALYZE ontology_triples;
ANALYZE crawl_jobs;
ANALYZE embedding_metadata;

-- ============================================================================
-- Comments for documentation
-- ============================================================================
COMMENT ON TABLE articles_raw IS 'Primary storage for crawled Naver news articles';
COMMENT ON TABLE comments_raw IS 'Storage for article comments with hierarchical structure';
COMMENT ON TABLE ontology_triples IS 'Knowledge graph triples extracted from articles (RDF-like)';
COMMENT ON TABLE crawl_jobs IS 'Job tracking for distributed crawling coordination';
COMMENT ON TABLE embedding_metadata IS 'Metadata for vector embeddings stored in OpenSearch';

COMMENT ON COLUMN articles_raw.oid IS 'Naver publisher/organization ID';
COMMENT ON COLUMN articles_raw.aid IS 'Naver article ID';
COMMENT ON COLUMN articles_raw.content_hash IS 'SHA256 hash for duplicate detection';
COMMENT ON COLUMN articles_raw.search_vector IS 'Auto-generated tsvector for full-text search';

COMMENT ON COLUMN comments_raw.comment_no IS 'Naver unique comment identifier';
COMMENT ON COLUMN comments_raw.parent_comment_no IS 'Parent comment for nested replies (NULL = top-level)';
COMMENT ON COLUMN comments_raw.depth IS 'Comment nesting depth (0 = top-level, 1+ = reply)';

COMMENT ON COLUMN ontology_triples.subject IS 'RDF subject - entity or concept';
COMMENT ON COLUMN ontology_triples.predicate IS 'RDF predicate - relationship type';
COMMENT ON COLUMN ontology_triples.object IS 'RDF object - related entity or value';
COMMENT ON COLUMN ontology_triples.confidence_score IS 'LLM extraction confidence (0.0-1.0)';

-- End of initialization script
