import { useState, useRef, useEffect } from 'react';
import { Search, ZoomIn, ZoomOut, Maximize2, Filter, Loader2, RefreshCw } from 'lucide-react';
import CytoscapeComponent from 'react-cytoscapejs';
import cytoscape from 'cytoscape';
import type { Core } from 'cytoscape';

interface Triple {
  subject: string;
  subject_type: string;
  predicate: string;
  predicate_label: string;
  object: string;
  object_type: string;
  confidence: number;
  evidence: string;
  article_id: string;
  article_title?: string;
}

interface OntologyData {
  stats: {
    total_articles: number;
    total_entities: number;
    total_triples: number;
    entity_types: Record<string, number>;
    relation_types: Record<string, number>;
  };
  triples: Triple[];
}

const nodeColors: Record<string, string> = {
  Person: '#3b82f6',
  Organization: '#10b981',
  Location: '#f59e0b',
  Statement: '#8b5cf6',
  Event: '#ef4444',
  Industry: '#06b6d4',
  Money: '#ec4899',
  Percentage: '#14b8a6',
  Other: '#6b7280',
  default: '#6b7280',
};

const relationLabels: Record<string, string> = {
  '소속': 'MemberOf',
  '위치': 'LocatedIn',
  '대표': 'Leads',
  '근무': 'WorksFor',
  '발언': 'Said',
  '투자': 'InvestedIn',
  '설립': 'Founded',
  '반대': 'Opposed',
  '비판': 'Criticized',
  '지지': 'Supported',
  '인수': 'Acquired',
  '소유': 'Owns',
};

export function Ontology() {
  const [searchQuery, setSearchQuery] = useState('');
  const [data, setData] = useState<OntologyData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [selectedRelationType, setSelectedRelationType] = useState<string>('all');
  const [displayLimit, setDisplayLimit] = useState(100);
  const cyRef = useRef<Core | null>(null);

  // Load ontology data
  useEffect(() => {
    const loadData = async () => {
      try {
        setLoading(true);
        const response = await fetch('/data/ontology-summary.json');
        if (!response.ok) {
          throw new Error('Failed to load ontology data');
        }
        const jsonData = await response.json();
        setData(jsonData);
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Unknown error');
      } finally {
        setLoading(false);
      }
    };

    loadData();
  }, []);

  // Filter triples based on search and relation type
  const filteredTriples = data?.triples.filter((triple) => {
    const matchesSearch =
      !searchQuery ||
      triple.subject.toLowerCase().includes(searchQuery.toLowerCase()) ||
      triple.object.toLowerCase().includes(searchQuery.toLowerCase());

    const matchesRelationType =
      selectedRelationType === 'all' ||
      triple.predicate_label === selectedRelationType;

    return matchesSearch && matchesRelationType;
  }).slice(0, displayLimit) || [];

  // Convert triples to cytoscape elements
  const elements = (() => {
    const nodes = new Map<string, { id: string; label: string; type: string }>();
    const edges: { source: string; target: string; label: string }[] = [];

    filteredTriples.forEach((triple) => {
      const subjectId = `${triple.subject_type}_${triple.subject}`.replace(/\s/g, '_');
      const objectId = `${triple.object_type}_${triple.object.slice(0, 30)}`.replace(/\s/g, '_');

      if (!nodes.has(subjectId)) {
        nodes.set(subjectId, {
          id: subjectId,
          label: triple.subject,
          type: triple.subject_type,
        });
      }

      if (!nodes.has(objectId)) {
        nodes.set(objectId, {
          id: objectId,
          label: triple.object.length > 30 ? triple.object.slice(0, 30) + '...' : triple.object,
          type: triple.object_type,
        });
      }

      edges.push({
        source: subjectId,
        target: objectId,
        label: triple.predicate_label,
      });
    });

    return [
      ...Array.from(nodes.values()).map((node) => ({
        data: node,
      })),
      ...edges.map((edge, i) => ({
        data: { ...edge, id: `edge_${i}` },
      })),
    ];
  })();

  const cyStylesheet = [
    {
      selector: 'node',
      style: {
        'background-color': (ele: cytoscape.NodeSingular) =>
          nodeColors[ele.data('type') as string] || nodeColors.default,
        label: 'data(label)',
        'text-valign': 'bottom',
        'text-halign': 'center',
        'font-size': '10px',
        'text-margin-y': 5,
        width: 40,
        height: 40,
      },
    },
    {
      selector: 'edge',
      style: {
        width: 2,
        'line-color': '#94a3b8',
        'target-arrow-color': '#94a3b8',
        'target-arrow-shape': 'triangle',
        'curve-style': 'bezier',
        label: 'data(label)',
        'font-size': '8px',
        'text-rotation': 'autorotate',
      },
    },
    {
      selector: 'node:selected',
      style: {
        'border-width': 3,
        'border-color': '#1e40af',
      },
    },
  ];

  const handleZoomIn = () => {
    if (cyRef.current) {
      cyRef.current.zoom(cyRef.current.zoom() * 1.2);
    }
  };

  const handleZoomOut = () => {
    if (cyRef.current) {
      cyRef.current.zoom(cyRef.current.zoom() * 0.8);
    }
  };

  const handleFit = () => {
    if (cyRef.current) {
      cyRef.current.fit();
    }
  };

  const handleRefresh = () => {
    if (cyRef.current) {
      cyRef.current.layout({ name: 'cose', animate: true } as cytoscape.LayoutOptions).run();
    }
  };

  if (loading) {
    return (
      <div className="p-6 h-full flex items-center justify-center">
        <div className="text-center">
          <Loader2 className="w-8 h-8 animate-spin text-blue-500 mx-auto mb-4" />
          <p className="text-gray-500">온톨로지 데이터 로딩 중...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6 h-full flex items-center justify-center">
        <div className="text-center">
          <p className="text-red-500 mb-2">데이터 로드 실패</p>
          <p className="text-gray-500">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 h-full flex flex-col">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Ontology</h1>
        <p className="text-gray-500">
          {data?.stats.total_articles.toLocaleString()}개 기사에서 추출된 지식 그래프
        </p>
      </div>

      <div className="flex-1 flex gap-6 min-h-0">
        {/* Graph View */}
        <div className="flex-1 bg-white rounded-xl shadow-sm overflow-hidden relative">
          {/* Controls */}
          <div className="absolute top-4 right-4 z-10 flex gap-2" role="toolbar" aria-label="그래프 컨트롤">
            <button
              onClick={handleRefresh}
              className="p-2 bg-white rounded-lg shadow hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500"
              aria-label="레이아웃 재배치"
            >
              <RefreshCw className="w-5 h-5" aria-hidden="true" />
            </button>
            <button
              onClick={handleZoomIn}
              className="p-2 bg-white rounded-lg shadow hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500"
              aria-label="확대"
            >
              <ZoomIn className="w-5 h-5" aria-hidden="true" />
            </button>
            <button
              onClick={handleZoomOut}
              className="p-2 bg-white rounded-lg shadow hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500"
              aria-label="축소"
            >
              <ZoomOut className="w-5 h-5" aria-hidden="true" />
            </button>
            <button
              onClick={handleFit}
              className="p-2 bg-white rounded-lg shadow hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500"
              aria-label="화면에 맞추기"
            >
              <Maximize2 className="w-5 h-5" aria-hidden="true" />
            </button>
          </div>

          {/* Search and Filter */}
          <div className="absolute top-4 left-4 z-10 flex gap-2" role="search" aria-label="온톨로지 검색">
            <div className="relative">
              <label htmlFor="ontology-search" className="sr-only">
                엔티티 검색
              </label>
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" aria-hidden="true" />
              <input
                id="ontology-search"
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="엔티티 검색..."
                className="pl-10 pr-4 py-2 w-48 rounded-lg border border-gray-200 focus:border-blue-500 outline-none text-sm focus:ring-2 focus:ring-blue-200"
              />
            </div>
            <div className="relative">
              <label htmlFor="relation-filter" className="sr-only">
                관계 유형 필터
              </label>
              <Filter className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" aria-hidden="true" />
              <select
                id="relation-filter"
                value={selectedRelationType}
                onChange={(e) => setSelectedRelationType(e.target.value)}
                className="pl-10 pr-4 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none text-sm appearance-none bg-white focus:ring-2 focus:ring-blue-200"
              >
                <option value="all">모든 관계</option>
                {Object.keys(data?.stats.relation_types || {}).map((type) => (
                  <option key={type} value={type}>
                    {type} ({relationLabels[type] || type})
                  </option>
                ))}
              </select>
            </div>
            <select
              value={displayLimit}
              onChange={(e) => setDisplayLimit(Number(e.target.value))}
              className="px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none text-sm appearance-none bg-white"
            >
              <option value={50}>50개</option>
              <option value={100}>100개</option>
              <option value={200}>200개</option>
              <option value={500}>500개</option>
            </select>
          </div>

          {/* Cytoscape Graph */}
          <CytoscapeComponent
            elements={elements}
            stylesheet={cyStylesheet as unknown as cytoscape.StylesheetCSS[]}
            style={{ width: '100%', height: '100%' }}
            layout={{ name: 'cose', animate: false }}
            cy={(cy: Core) => {
              cyRef.current = cy;
              cy.on('tap', 'node', (evt: cytoscape.EventObject) => {
                setSelectedNode(evt.target.data('label'));
              });
            }}
          />

          {/* Legend */}
          <div className="absolute bottom-4 left-4 bg-white p-3 rounded-lg shadow">
            <p className="text-xs font-medium text-gray-500 mb-2">범례</p>
            <div className="flex flex-wrap gap-3">
              {Object.entries(nodeColors)
                .filter(([key]) => key !== 'default')
                .map(([type, color]) => (
                  <div key={type} className="flex items-center gap-1">
                    <div
                      className="w-3 h-3 rounded-full"
                      style={{ backgroundColor: color }}
                    />
                    <span className="text-xs text-gray-600">{type}</span>
                  </div>
                ))}
            </div>
          </div>

          {/* Display count */}
          <div className="absolute bottom-4 right-4 bg-white px-3 py-1 rounded-lg shadow text-sm text-gray-500">
            표시 중: {filteredTriples.length}개 관계
          </div>
        </div>

        {/* Side Panel */}
        <div className="w-80 space-y-4 overflow-y-auto">
          {/* Stats */}
          <div className="bg-white rounded-xl shadow-sm p-4">
            <h3 className="font-semibold mb-3">통계</h3>
            <div className="space-y-2">
              <div className="flex justify-between">
                <span className="text-gray-500">전체 기사</span>
                <span className="font-medium">
                  {data?.stats.total_articles.toLocaleString()}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">전체 엔티티</span>
                <span className="font-medium">
                  {data?.stats.total_entities.toLocaleString()}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-500">전체 관계</span>
                <span className="font-medium">
                  {data?.stats.total_triples.toLocaleString()}
                </span>
              </div>
            </div>

            <div className="mt-4">
              <p className="text-sm text-gray-500 mb-2">엔티티 타입</p>
              {Object.entries(data?.stats.entity_types || {})
                .sort(([, a], [, b]) => b - a)
                .map(([type, count]) => (
                  <div key={type} className="flex items-center gap-2 mb-1">
                    <div
                      className="w-2 h-2 rounded-full"
                      style={{
                        backgroundColor: nodeColors[type] || nodeColors.default,
                      }}
                    />
                    <span className="text-sm flex-1">{type}</span>
                    <span className="text-sm text-gray-500">
                      {count.toLocaleString()}
                    </span>
                  </div>
                ))}
            </div>

            <div className="mt-4">
              <p className="text-sm text-gray-500 mb-2">관계 타입</p>
              {Object.entries(data?.stats.relation_types || {})
                .sort(([, a], [, b]) => b - a)
                .map(([type, count]) => (
                  <div key={type} className="flex justify-between text-sm mb-1">
                    <span>
                      {type}
                      <span className="text-gray-400 ml-1 text-xs">
                        ({relationLabels[type] || type})
                      </span>
                    </span>
                    <span className="text-gray-500">{count.toLocaleString()}</span>
                  </div>
                ))}
            </div>
          </div>

          {/* Selected Node Info */}
          {selectedNode && (
            <div className="bg-white rounded-xl shadow-sm p-4">
              <h3 className="font-semibold mb-3">선택된 노드</h3>
              <p className="text-lg font-medium text-blue-600">{selectedNode}</p>
              <div className="mt-3 space-y-2 max-h-60 overflow-y-auto">
                {filteredTriples
                  .filter(
                    (t) =>
                      t.subject === selectedNode ||
                      t.object.includes(selectedNode)
                  )
                  .slice(0, 10)
                  .map((t, i) => (
                    <div key={i} className="text-sm p-2 bg-gray-50 rounded">
                      <div>
                        <span className="font-medium">{t.subject}</span>
                        <span className="text-gray-500">
                          {' '}
                          → {t.predicate_label} →{' '}
                        </span>
                        <span>
                          {t.object.length > 30
                            ? t.object.slice(0, 30) + '...'
                            : t.object}
                        </span>
                      </div>
                      {t.article_title && (
                        <p className="text-xs text-gray-400 mt-1 truncate">
                          출처: {t.article_title}
                        </p>
                      )}
                    </div>
                  ))}
              </div>
            </div>
          )}

          {/* Recent Triples */}
          <div className="bg-white rounded-xl shadow-sm p-4">
            <h3 className="font-semibold mb-3">최근 추출 관계</h3>
            <div className="space-y-2 max-h-60 overflow-y-auto">
              {filteredTriples.slice(0, 10).map((t, i) => (
                <div
                  key={i}
                  className="text-sm p-2 bg-gray-50 rounded hover:bg-gray-100 cursor-pointer"
                  onClick={() => setSelectedNode(t.subject)}
                >
                  <div className="flex items-center gap-1">
                    <span
                      className="w-2 h-2 rounded-full"
                      style={{
                        backgroundColor:
                          nodeColors[t.subject_type] || nodeColors.default,
                      }}
                    />
                    <span className="font-medium">{t.subject}</span>
                    <span className="text-gray-400">→</span>
                    <span className="text-blue-600">{t.predicate_label}</span>
                    <span className="text-gray-400">→</span>
                    <span className="truncate">{t.object}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
