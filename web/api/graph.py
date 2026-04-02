"""
Knowledge Graph API Server
Provides REST endpoints for graph visualization
"""
from fastapi import FastAPI, Query, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from typing import Optional, List
import psycopg2
from psycopg2.extras import RealDictCursor
from contextlib import contextmanager
import json

app = FastAPI(title="Baram Knowledge Graph API", version="1.0.0")

# CORS for frontend
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Database configuration
DB_CONFIG = {
    "host": "localhost",
    "port": 5432,
    "dbname": "baram",
    "user": "baram",
    "password": "momo00"
}

@contextmanager
def get_db():
    """Get database connection with context manager"""
    conn = psycopg2.connect(**DB_CONFIG, cursor_factory=RealDictCursor)
    try:
        yield conn
    finally:
        conn.close()


@app.get("/api/graph/entities")
def get_entities(
    limit: int = Query(100, ge=1, le=1000),
    offset: int = Query(0, ge=0),
    entity_type: Optional[str] = None
):
    """Get list of entities with pagination"""
    with get_db() as conn:
        cur = conn.cursor()
        
        query = "SELECT id, name, type, mention_count, created_at FROM kg_entities"
        params = []
        
        if entity_type:
            query += " WHERE type = %s"
            params.append(entity_type)
        
        query += " ORDER BY mention_count DESC LIMIT %s OFFSET %s"
        params.extend([limit, offset])
        
        cur.execute(query, params)
        entities = cur.fetchall()
        
        # Get total count
        count_query = "SELECT COUNT(*) as total FROM kg_entities"
        if entity_type:
            count_query += " WHERE type = %s"
            cur.execute(count_query, [entity_type] if entity_type else [])
        else:
            cur.execute(count_query)
        total = cur.fetchone()["total"]
        
        # Get entity types for filter
        cur.execute("SELECT DISTINCT type, COUNT(*) as count FROM kg_entities GROUP BY type ORDER BY count DESC")
        entity_types = cur.fetchall()
        
        return {
            "entities": [dict(e) for e in entities],
            "total": total,
            "limit": limit,
            "offset": offset,
            "entity_types": [dict(t) for t in entity_types]
        }


@app.get("/api/graph/relations")
def get_relations(
    entity_id: Optional[int] = None,
    limit: int = Query(100, ge=1, le=1000)
):
    """Get relations for a specific entity or all relations"""
    with get_db() as conn:
        cur = conn.cursor()
        
        if entity_id:
            query = """
                SELECT r.id, r.source_id, r.target_id, r.relation, r.confidence, r.article_id,
                       s.name as source_name, s.type as source_type,
                       t.name as target_name, t.type as target_type
                FROM kg_relations r
                JOIN kg_entities s ON r.source_id = s.id
                JOIN kg_entities t ON r.target_id = t.id
                WHERE r.source_id = %s OR r.target_id = %s
                ORDER BY r.confidence DESC
                LIMIT %s
            """
            cur.execute(query, [entity_id, entity_id, limit])
        else:
            query = """
                SELECT r.id, r.source_id, r.target_id, r.relation, r.confidence, r.article_id,
                       s.name as source_name, s.type as source_type,
                       t.name as target_name, t.type as target_type
                FROM kg_relations r
                JOIN kg_entities s ON r.source_id = s.id
                JOIN kg_entities t ON r.target_id = t.id
                ORDER BY r.confidence DESC
                LIMIT %s
            """
            cur.execute(query, [limit])
        
        relations = cur.fetchall()
        
        # Get relation types
        cur.execute("SELECT DISTINCT relation, COUNT(*) as count FROM kg_relations GROUP BY relation ORDER BY count DESC")
        relation_types = cur.fetchall()
        
        return {
            "relations": [dict(r) for r in relations],
            "relation_types": [dict(t) for t in relation_types]
        }


@app.get("/api/graph/search")
def search_entities(
    q: str = Query(..., min_length=1),
    limit: int = Query(20, ge=1, le=100)
):
    """Search entities by name"""
    with get_db() as conn:
        cur = conn.cursor()
        
        query = """
            SELECT id, name, type, mention_count
            FROM kg_entities
            WHERE name ILIKE %s
            ORDER BY mention_count DESC
            LIMIT %s
        """
        cur.execute(query, [f"%{q}%", limit])
        entities = cur.fetchall()
        
        return {
            "query": q,
            "results": [dict(e) for e in entities]
        }


@app.get("/api/graph/subgraph")
def get_subgraph(
    entity_id: int = Query(...),
    depth: int = Query(1, ge=1, le=3)
):
    """Get subgraph centered on an entity with specified depth"""
    with get_db() as conn:
        cur = conn.cursor()
        
        visited_entities = set()
        edges = []
        
        def explore(eid, current_depth):
            if current_depth > depth or eid in visited_entities:
                return
            visited_entities.add(eid)
            
            cur.execute("""
                SELECT r.id, r.source_id, r.target_id, r.relation, r.confidence
                FROM kg_relations r
                WHERE r.source_id = %s OR r.target_id = %s
            """, [eid, eid])
            
            for row in cur.fetchall():
                edges.append(dict(row))
                next_id = row["target_id"] if row["source_id"] == eid else row["source_id"]
                explore(next_id, current_depth + 1)
        
        explore(entity_id, 1)
        
        # Get all visited entities
        if visited_entities:
            cur.execute("""
                SELECT id, name, type, mention_count
                FROM kg_entities
                WHERE id = ANY(%s)
            """, [list(visited_entities)])
            nodes = [dict(e) for e in cur.fetchall()]
        else:
            nodes = []
        
        return {
            "center_entity_id": entity_id,
            "depth": depth,
            "nodes": nodes,
            "edges": edges
        }


@app.get("/api/graph/stats")
def get_graph_stats():
    """Get overall graph statistics"""
    with get_db() as conn:
        cur = conn.cursor()
        
        cur.execute("SELECT COUNT(*) as total FROM kg_entities")
        total_entities = cur.fetchone()["total"]
        
        cur.execute("SELECT COUNT(*) as total FROM kg_relations")
        total_relations = cur.fetchone()["total"]
        
        cur.execute("SELECT type, COUNT(*) as count FROM kg_entities GROUP BY type ORDER BY count DESC")
        entity_types = [dict(r) for r in cur.fetchall()]
        
        cur.execute("SELECT relation, COUNT(*) as count FROM kg_relations GROUP BY relation ORDER BY count DESC")
        relation_types = [dict(r) for r in cur.fetchall()]
        
        cur.execute("SELECT name, type, mention_count FROM kg_entities ORDER BY mention_count DESC LIMIT 10")
        top_entities = [dict(r) for r in cur.fetchall()]
        
        return {
            "total_entities": total_entities,
            "total_relations": total_relations,
            "entity_types": entity_types,
            "relation_types": relation_types,
            "top_entities": top_entities
        }


@app.get("/api/graph/full")
def get_full_graph(limit: int = Query(500, ge=1, le=2000)):
    """Get full graph data for Sigma.js visualization"""
    with get_db() as conn:
        cur = conn.cursor()
        
        # Get top entities by mention count
        cur.execute("""
            SELECT id, name, type, mention_count
            FROM kg_entities
            ORDER BY mention_count DESC
            LIMIT %s
        """, [limit])
        entities = [dict(e) for e in cur.fetchall()]
        entity_ids = [e["id"] for e in entities]
        
        if not entity_ids:
            return {"nodes": [], "edges": []}
        
        # Get relations between these entities
        cur.execute("""
            SELECT r.id, r.source_id, r.target_id, r.relation, r.confidence
            FROM kg_relations r
            WHERE r.source_id = ANY(%s) AND r.target_id = ANY(%s)
        """, [entity_ids, entity_ids])
        relations = [dict(r) for r in cur.fetchall()]
        
        # Format for Sigma.js
        nodes = [
            {
                "id": str(e["id"]),
                "label": e["name"],
                "type": "circle",
                "entity_type": e["type"],
                "size": max(5, min(30, e["mention_count"] * 2)),
                "x": 0,  # Will be laid out by Sigma
                "y": 0
            }
            for e in entities
        ]
        
        edges = [
            {
                "id": f"e{r["id"]}",
                "source": str(r["source_id"]),
                "target": str(r["target_id"]),
                "label": r["relation"],
                "weight": r["confidence"]
            }
            for r in relations
        ]
        
        return {"nodes": nodes, "edges": edges}




# ========================================
# Dashboard & System API endpoints
# ========================================

import subprocess
import os
import requests as http_requests
from datetime import datetime, timedelta

OPENSEARCH_URL = os.getenv("OPENSEARCH_URL", "http://localhost:9200")
OPENSEARCH_INDEX = "baram-articles"


@app.get("/api/stats")
def get_crawl_stats():
    """Dashboard crawl statistics"""
    try:
        r = http_requests.get(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_count", timeout=5)
        total = r.json().get("count", 0) if r.ok else 0

        today = datetime.utcnow().strftime("%Y-%m-%d")
        today_query = {"query": {"range": {"crawled_at": {"gte": f"{today}T00:00:00"}}}}
        r2 = http_requests.post(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_count", json=today_query, timeout=5)
        today_count = r2.json().get("count", 0) if r2.ok else 0

        cat_query = {"size": 0, "aggs": {"cats": {"terms": {"field": "category", "size": 20}}}}
        r3 = http_requests.post(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_search", json=cat_query, timeout=5)
        categories = {}
        if r3.ok:
            for b in r3.json().get("aggregations", {}).get("cats", {}).get("buckets", []):
                categories[b["key"]] = b["doc_count"]

        pub_query = {"size": 0, "aggs": {"pubs": {"terms": {"field": "publisher", "size": 20}}}}
        r4 = http_requests.post(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_search", json=pub_query, timeout=5)
        publishers = {}
        if r4.ok:
            for b in r4.json().get("aggregations", {}).get("pubs", {}).get("buckets", []):
                publishers[b["key"]] = b["doc_count"]

        daily_query = {
            "size": 0,
            "aggs": {"daily": {"date_histogram": {"field": "crawled_at", "calendar_interval": "day", "format": "yyyy-MM-dd"}}}
        }
        r5 = http_requests.post(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_search", json=daily_query, timeout=5)
        daily_counts = []
        if r5.ok:
            for b in r5.json().get("aggregations", {}).get("daily", {}).get("buckets", [])[-7:]:
                daily_counts.append({"date": b["key_as_string"], "count": b["doc_count"]})

        # Hourly counts (last 24h)
        hourly_query = {
            "size": 0,
            "query": {"range": {"crawled_at": {"gte": "now-24h"}}},
            "aggs": {"hourly": {"date_histogram": {"field": "crawled_at", "calendar_interval": "hour", "format": "HH:mm"}}}
        }
        r6 = http_requests.post(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_search", json=hourly_query, timeout=5)
        hourly_counts = []
        if r6.ok:
            for b in r6.json().get("aggregations", {}).get("hourly", {}).get("buckets", []):
                hourly_counts.append({"hour": b["key_as_string"], "count": b["doc_count"]})

        return {
            "total_articles": total,
            "today_articles": today_count,
            "categories": categories,
            "publishers": publishers,
            "hourly_counts": hourly_counts,
            "daily_counts": daily_counts
        }
    except Exception as e:
        return {"total_articles": 0, "today_articles": 0, "categories": {}, "publishers": {}, "hourly_counts": [], "daily_counts": [], "error": str(e)}


@app.get("/api/status")
def get_system_status():
    """System health status"""
    db_ok = "healthy"
    try:
        with get_db() as conn:
            cur = conn.cursor()
            cur.execute("SELECT 1")
    except Exception:
        db_ok = "unhealthy"

    llm_endpoint = os.getenv("LLM_ENDPOINT", "http://10.100.3.30:8000")
    llm_status = "unavailable"
    try:
        r = http_requests.get(f"{llm_endpoint}/health", timeout=3)
        data = r.json() if r.ok else {}; llm_status = "healthy" if r.ok and data.get("services", {}).get("llm") == "up" else "unhealthy"
    except Exception:
        llm_status = "unavailable"

    try:
        st = os.statvfs("/data")
        disk_usage = round((1 - st.f_bavail / st.f_blocks) * 100, 1)
    except Exception:
        disk_usage = 0

    try:
        with open("/proc/uptime") as f:
            uptime = int(float(f.read().split()[0]))
    except Exception:
        uptime = 0

    return {"database": db_ok, "llm": llm_status, "disk_usage": disk_usage, "uptime": uptime}


@app.get("/api/articles/recent")
def get_recent_articles(limit: int = Query(10, ge=1, le=50)):
    """Get recent articles from OpenSearch"""
    try:
        query = {
            "size": limit,
            "sort": [{"crawled_at": {"order": "desc"}}],
            "_source": ["title", "url", "category", "publisher", "crawled_at", "published_at", "content", "author"]
        }
        r = http_requests.post(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_search", json=query, timeout=5)
        if r.ok:
            hits = r.json().get("hits", {}).get("hits", [])
            articles = []
            for h in hits:
                src = h["_source"]
                src["id"] = h["_id"]
                articles.append(src)
            return {"articles": articles, "total": len(articles)}
        return {"articles": [], "total": 0}
    except Exception as e:
        return {"articles": [], "total": 0, "error": str(e)}


@app.get("/api/ontology/stats")
def get_ontology_stats():
    """Ontology statistics"""
    stats = get_graph_stats()

    total_articles = 0
    try:
        r = http_requests.get(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_count", timeout=5)
        total_articles = r.json().get("count", 0) if r.ok else 0
    except Exception:
        pass

    entity_types = {et["type"]: et["count"] for et in stats.get("entity_types", [])}
    relation_types = {rt["relation"]: rt["count"] for rt in stats.get("relation_types", [])}

    return {
        "total_articles": total_articles,
        "total_entities": stats.get("total_entities", 0),
        "total_triples": stats.get("total_relations", 0),
        "entity_types": entity_types,
        "relation_types": relation_types
    }


@app.get("/api/search")
def search_articles(
    q: str = Query(""),
    category: Optional[str] = None,
    publisher: Optional[str] = None,
    date_from: Optional[str] = Query(None, alias="from"),
    date_to: Optional[str] = Query(None, alias="to"),
    page: int = Query(1, ge=1),
    limit: int = Query(20, ge=1, le=100)
):
    """Search articles in OpenSearch"""
    try:
        must = []
        if q:
            must.append({"multi_match": {"query": q, "fields": ["title^3", "content"]}})
        if category:
            must.append({"term": {"category": category}})
        if publisher:
            must.append({"term": {"publisher": publisher}})
        if date_from or date_to:
            range_q = {}
            if date_from:
                range_q["gte"] = date_from
            if date_to:
                range_q["lte"] = date_to
            must.append({"range": {"published_at": range_q}})

        body = {
            "from": (page - 1) * limit,
            "size": limit,
            "sort": [{"crawled_at": {"order": "desc"}}],
            "query": {"bool": {"must": must}} if must else {"match_all": {}}
        }

        r = http_requests.post(f"{OPENSEARCH_URL}/{OPENSEARCH_INDEX}/_search", json=body, timeout=10)
        if r.ok:
            data = r.json()
            total = data["hits"]["total"]["value"]
            articles = []
            for h in data["hits"]["hits"]:
                src = h["_source"]
                src["id"] = h["_id"]
                articles.append(src)
            return {"articles": articles, "total": total, "page": page, "limit": limit}
        return {"articles": [], "total": 0, "page": page, "limit": limit}
    except Exception as e:
        return {"articles": [], "total": 0, "page": page, "limit": limit, "error": str(e)}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8080)
