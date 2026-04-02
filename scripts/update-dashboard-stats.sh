#\!/bin/bash
COUNT=$(curl -s http://localhost:9200/baram-articles/_count | jq -r ".count")
if [ -n "$COUNT" ] && [ "$COUNT" \!= "null" ]; then
    sed -i "s/\"total_articles\": [0-9]*/\"total_articles\": $COUNT/" /data/Baram/web/dist/data/ontology-summary.json
    echo "$(date): Updated to $COUNT"
fi
