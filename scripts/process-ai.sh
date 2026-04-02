#!/bin/bash
# Baram AI Pipeline - Entity/Relation Extraction

set -e

AI_GATEWAY="http://10.100.3.30:8000"
POSTGRES_CONTAINER=$(docker ps -qf name=postgres)
RAW_DIR="/data/Baram/output/raw"
LOG_FILE="/data/Baram/logs/ai_process_$(date +%Y%m%d_%H%M%S).log"
PROCESSED_STATE="/data/Baram/scripts/.ai_processed_state"
MAX_ARTICLES=${MAX_ARTICLES:-20}
MAX_TOKENS=5000
FORCE_REPROCESS=${FORCE_REPROCESS:-0}

log() {
    echo "[$(date +"%Y-%m-%d %H:%M:%S")] $1" | tee -a "$LOG_FILE"
}

psql_exec() {
    docker exec -i "$POSTGRES_CONTAINER" psql -U baram -d baram -t -A -c "$1" 2>/dev/null
}

upsert_entity() {
    local name="$1"
    local type="$2"
    name=$(echo "$name" | sed "s/'/''/g")
    psql_exec "INSERT INTO kg_entities (name, type, mention_count) 
               VALUES ('$name', '$type', 1) 
               ON CONFLICT (name, type) 
               DO UPDATE SET mention_count = kg_entities.mention_count + 1 
               RETURNING id;"
}

insert_relation() {
    local source_id="$1"
    local target_id="$2"
    local relation="$3"
    local confidence="$4"
    local article_id="$5"
    relation=$(echo "$relation" | sed "s/'/''/g")
    psql_exec "INSERT INTO kg_relations (source_id, target_id, relation, confidence, article_id) 
               VALUES ($source_id, $target_id, '$relation', $confidence, '$article_id')
               ON CONFLICT DO NOTHING;"
}

process_article() {
    local article_file="$1"
    local article_id=$(basename "$article_file" .md | cut -d_ -f1-2)
    
    log "Processing: $article_id"
    
    local content=$(sed -n '19,$p' "$article_file" | head -n -3 | head -15)
    
    if [ -z "$content" ]; then
        log "  Skip: No content"
        return 1
    fi
    
    log "  Content: $(echo "$content" | head -c 50)..."
    
    # Shorter content
    local truncated=$(echo "$content" | head -c 400 | tr '\n' ' ')
    
    local prompt="${truncated}

개체와 관계 추출. JSON: {entities:[],relations:[]}"
    
    local escaped_prompt=$(printf '%s' "$prompt" | jq -Rs .)
    local request="{\"model\":\"qwen3.5-35b\",\"messages\":[{\"role\":\"user\",\"content\":${escaped_prompt}}],\"max_tokens\":${MAX_TOKENS},\"temperature\":0.1}"
    
    local response=$(curl -s -X POST "$AI_GATEWAY/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d "$request" \
        --max-time 300)
    
    local llm_content=$(echo "$response" | jq -r '.choices[0].message.content // empty')
    
    if [ -z "$llm_content" ]; then
        log "  Warning: Empty LLM response"
        return 1
    fi
    
    local json=$(echo "$llm_content" | sed 's/```json//g;s/```//g' | tr '\n' ' ')
    log "  LLM OK: $(echo "$json" | head -c 80)..."
    
    # Save JSON to temp file
    local tmp_json=$(mktemp)
    echo "$json" > "$tmp_json"
    
    # Process entities - handle both formats:
    # 1. Array of objects: [{name:"...",type:"..."}]
    # 2. Array of strings: ["entity1", "entity2"]
    local entity_count=0
    local entities_tmp=$(mktemp)
    
    # Check if entities is array of strings or objects
    local first_entity=$(jq -r '.entities[0] | type' < "$tmp_json" 2>/dev/null)
    
    if [ "$first_entity" = "string" ]; then
        # Array of strings - treat as unknown type
        jq -r '.entities[]?' < "$tmp_json" 2>/dev/null > "$entities_tmp" || true
        while IFS= read -r name; do
            if [ -n "$name" ] && [ "$name" != "null" ]; then
                local eid=$(upsert_entity "$name" "UNKNOWN")
                if [ -n "$eid" ]; then
                    log "    Entity: $name (UNKNOWN) -> $eid"
                    entity_count=$((entity_count + 1))
                fi
            fi
        done < "$entities_tmp"
    else
        # Array of objects
        jq -c '.entities[]?' < "$tmp_json" 2>/dev/null > "$entities_tmp" || true
        while IFS= read -r entity; do
            if [ -n "$entity" ]; then
                local name=$(echo "$entity" | jq -r '.text // .name // empty')
                local type=$(echo "$entity" | jq -r '.type // "UNKNOWN"')
                if [ -n "$name" ] && [ "$name" != "null" ]; then
                    local eid=$(upsert_entity "$name" "$type")
                    if [ -n "$eid" ]; then
                        log "    Entity: $name ($type) -> $eid"
                        entity_count=$((entity_count + 1))
                    fi
                fi
            fi
        done < "$entities_tmp"
    fi
    
    # Process relations
    local rel_count=0
    local relations_tmp=$(mktemp)
    jq -c '.relations[]?' < "$tmp_json" 2>/dev/null > "$relations_tmp" || true
    
    while IFS= read -r rel; do
        if [ -n "$rel" ]; then
            local src=$(echo "$rel" | jq -r '.source // empty')
            local tgt=$(echo "$rel" | jq -r '.target // empty')
            local rtype=$(echo "$rel" | jq -r '.type // .relation // "RELATED"')
            local conf=$(echo "$rel" | jq -r '.confidence // 0.8')
            
            if [ -n "$src" ] && [ "$src" != "null" ] && [ -n "$tgt" ] && [ "$tgt" != "null" ]; then
                local src_id=$(psql_exec "SELECT id FROM kg_entities WHERE name='$(echo "$src" | sed "s/'/''/g")' LIMIT 1;")
                local tgt_id=$(psql_exec "SELECT id FROM kg_entities WHERE name='$(echo "$tgt" | sed "s/'/''/g")' LIMIT 1;")
                if [ -n "$src_id" ] && [ -n "$tgt_id" ]; then
                    insert_relation "$src_id" "$tgt_id" "$rtype" "$conf" "$article_id"
                    log "    Relation: $src -[$rtype]-> $tgt"
                    rel_count=$((rel_count + 1))
                fi
            fi
        fi
    done < "$relations_tmp"
    
    rm -f "$tmp_json" "$entities_tmp" "$relations_tmp"
    
    log "  Done: $entity_count entities, $rel_count relations"
    return 0
}

main() {
    log "=== Baram AI Pipeline Start ==="
    log "AI Gateway: $AI_GATEWAY"
    log "Max articles: $MAX_ARTICLES"
    
    if ! docker exec -i "$POSTGRES_CONTAINER" pg_isready -U baram >/dev/null 2>&1; then
        log "ERROR: PostgreSQL not ready"
        exit 1
    fi
    log "PostgreSQL: OK"
    
    local health=$(curl -s "$AI_GATEWAY/health" 2>/dev/null || echo "{}")
    local llm_status=$(echo "$health" | jq -r '.services.llm // "down"')
    if [ "$llm_status" != "up" ]; then
        log "ERROR: LLM service down"
        exit 1
    fi
    log "AI Gateway: OK"
    
    if [ "$FORCE_REPROCESS" = "1" ] || [ ! -f "$PROCESSED_STATE" ]; then
        touch -d "2024-01-01" "$PROCESSED_STATE"
        log "State file reset"
    fi
    
    local tmp_list=$(mktemp)
    find "$RAW_DIR" -name "*.md" -newer "$PROCESSED_STATE" 2>/dev/null | head -$MAX_ARTICLES > "$tmp_list"
    local article_count=$(wc -l < "$tmp_list")
    
    log "Found $article_count articles"
    
    local processed=0
    local success=0
    
    while IFS= read -r article_file; do
        if [ -n "$article_file" ]; then
            if process_article "$article_file"; then
                success=$((success + 1))
            fi
            processed=$((processed + 1))
            sleep 3
        fi
    done < "$tmp_list"
    
    rm -f "$tmp_list"
    touch "$PROCESSED_STATE"
    
    local total_e=$(psql_exec "SELECT COUNT(*) FROM kg_entities;")
    local total_r=$(psql_exec "SELECT COUNT(*) FROM kg_relations;")
    
    log ""
    log "=== AI Pipeline Complete ==="
    log "Processed: $processed, Success: $success"
    log "Total entities: $total_e, relations: $total_r"
    log "=============================="
}

main "$@"
