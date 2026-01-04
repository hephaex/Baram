// Article types
export interface Article {
  id: string;
  oid: string;
  aid: string;
  title: string;
  content: string;
  url: string;
  category: string;
  publisher: string;
  author?: string;
  published_at: string;
  crawled_at: string;
}

// Crawl statistics
export interface CrawlStats {
  total_articles: number;
  today_articles: number;
  categories: Record<string, number>;
  publishers: Record<string, number>;
  hourly_counts: { hour: string; count: number }[];
  daily_counts: { date: string; count: number }[];
}

// System status
export interface SystemStatus {
  database: 'healthy' | 'unhealthy';
  llm: 'healthy' | 'unhealthy' | 'unavailable';
  disk_usage: number;
  uptime: number;
}

// Search types
export interface SearchParams {
  query: string;
  category?: string;
  publisher?: string;
  date_from?: string;
  date_to?: string;
  page?: number;
  limit?: number;
}

export interface SearchResult {
  articles: Article[];
  total: number;
  page: number;
  limit: number;
}

// Ontology types
export interface Triple {
  subject: string;
  subject_type: string;
  predicate: string;
  predicate_label: string;
  object: string;
  object_type: string;
  confidence: number;
  evidence: string;
  article_id: string;
}

export interface OntologyStats {
  total_articles: number;
  total_entities: number;
  total_triples: number;
  entity_types: Record<string, number>;
  relation_types: Record<string, number>;
}

export interface GraphNode {
  id: string;
  label: string;
  type: string;
}

export interface GraphEdge {
  source: string;
  target: string;
  label: string;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}
