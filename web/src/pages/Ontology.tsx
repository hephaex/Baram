import { useState, useRef } from 'react';
import { Search, ZoomIn, ZoomOut, Maximize2 } from 'lucide-react';
import CytoscapeComponent from 'react-cytoscapejs';
import cytoscape from 'cytoscape';
import type { Core } from 'cytoscape';
import type { Triple, OntologyStats } from '../types';

// Demo data
const demoTriples: Triple[] = [
  {
    subject: '김수종',
    subject_type: 'Person',
    predicate: 'schema:author',
    predicate_label: '발언',
    object: '첫 상업 발사는 기술적 완성도뿐 아니라 재현 가능한 신뢰와 안전 체계를 동시에 입증해야 하는 가장 높은 문턱',
    object_type: 'Statement',
    confidence: 0.9,
    evidence: '김수종 이노스페이스 대표는',
    article_id: '001_0000001',
  },
  {
    subject: '김수종',
    subject_type: 'Person',
    predicate: 'schema:memberOf',
    predicate_label: '소속',
    object: '이노스페이스',
    object_type: 'Organization',
    confidence: 0.95,
    evidence: '김수종 이노스페이스 대표',
    article_id: '001_0000001',
  },
  {
    subject: '이노스페이스',
    subject_type: 'Organization',
    predicate: 'schema:industry',
    predicate_label: '업종',
    object: '우주항공',
    object_type: 'Industry',
    confidence: 0.85,
    evidence: '우주발사체 기업 이노스페이스',
    article_id: '001_0000001',
  },
  {
    subject: '박영수',
    subject_type: 'Person',
    predicate: 'schema:author',
    predicate_label: '발언',
    object: '이번 발사는 한국 우주산업의 중요한 이정표',
    object_type: 'Statement',
    confidence: 0.9,
    evidence: '박영수 전문가는',
    article_id: '001_0000001',
  },
];

const demoStats: OntologyStats = {
  total_triples: 15234,
  entity_types: {
    'Person': 3421,
    'Organization': 2134,
    'Location': 1532,
    'Event': 987,
  },
  relation_types: {
    '발언': 5234,
    '소속': 3421,
    '참석': 1234,
    '위치': 987,
  },
};

const nodeColors: Record<string, string> = {
  Person: '#3b82f6',
  Organization: '#10b981',
  Location: '#f59e0b',
  Statement: '#8b5cf6',
  Event: '#ef4444',
  Industry: '#06b6d4',
  default: '#6b7280',
};

export function Ontology() {
  const [searchQuery, setSearchQuery] = useState('');
  const [stats] = useState<OntologyStats>(demoStats);
  const [triples] = useState<Triple[]>(demoTriples);
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const cyRef = useRef<Core | null>(null);

  // Convert triples to cytoscape elements
  const elements = (() => {
    const nodes = new Map<string, { id: string; label: string; type: string }>();
    const edges: { source: string; target: string; label: string }[] = [];

    triples.forEach((triple) => {
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
        'background-color': (ele: any) =>
          nodeColors[ele.data('type')] || nodeColors.default,
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

  return (
    <div className="p-6 h-full flex flex-col">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Ontology</h1>
        <p className="text-gray-500">엔티티 관계 그래프 시각화</p>
      </div>

      <div className="flex-1 flex gap-6">
        {/* Graph View */}
        <div className="flex-1 bg-white rounded-xl shadow-sm overflow-hidden relative">
          {/* Controls */}
          <div className="absolute top-4 right-4 z-10 flex gap-2">
            <button
              onClick={handleZoomIn}
              className="p-2 bg-white rounded-lg shadow hover:bg-gray-50"
            >
              <ZoomIn className="w-5 h-5" />
            </button>
            <button
              onClick={handleZoomOut}
              className="p-2 bg-white rounded-lg shadow hover:bg-gray-50"
            >
              <ZoomOut className="w-5 h-5" />
            </button>
            <button
              onClick={handleFit}
              className="p-2 bg-white rounded-lg shadow hover:bg-gray-50"
            >
              <Maximize2 className="w-5 h-5" />
            </button>
          </div>

          {/* Search */}
          <div className="absolute top-4 left-4 z-10">
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="엔티티 검색..."
                className="pl-10 pr-4 py-2 w-64 rounded-lg border border-gray-200 focus:border-blue-500 outline-none text-sm"
              />
            </div>
          </div>

          {/* Cytoscape Graph */}
          <CytoscapeComponent
            elements={elements}
            stylesheet={cyStylesheet as any}
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
        </div>

        {/* Side Panel */}
        <div className="w-80 space-y-4">
          {/* Stats */}
          <div className="bg-white rounded-xl shadow-sm p-4">
            <h3 className="font-semibold mb-3">통계</h3>
            <div className="space-y-2">
              <div className="flex justify-between">
                <span className="text-gray-500">전체 트리플</span>
                <span className="font-medium">
                  {stats.total_triples.toLocaleString()}
                </span>
              </div>
            </div>

            <div className="mt-4">
              <p className="text-sm text-gray-500 mb-2">엔티티 타입</p>
              {Object.entries(stats.entity_types).map(([type, count]) => (
                <div key={type} className="flex items-center gap-2 mb-1">
                  <div
                    className="w-2 h-2 rounded-full"
                    style={{
                      backgroundColor: nodeColors[type] || nodeColors.default,
                    }}
                  />
                  <span className="text-sm flex-1">{type}</span>
                  <span className="text-sm text-gray-500">{count}</span>
                </div>
              ))}
            </div>

            <div className="mt-4">
              <p className="text-sm text-gray-500 mb-2">관계 타입</p>
              {Object.entries(stats.relation_types).map(([type, count]) => (
                <div key={type} className="flex justify-between text-sm mb-1">
                  <span>{type}</span>
                  <span className="text-gray-500">{count}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Selected Node Info */}
          {selectedNode && (
            <div className="bg-white rounded-xl shadow-sm p-4">
              <h3 className="font-semibold mb-3">선택된 노드</h3>
              <p className="text-lg font-medium text-blue-600">{selectedNode}</p>
              <div className="mt-3 space-y-2">
                {triples
                  .filter(
                    (t) => t.subject === selectedNode || t.object.includes(selectedNode)
                  )
                  .map((t, i) => (
                    <div key={i} className="text-sm p-2 bg-gray-50 rounded">
                      <span className="font-medium">{t.subject}</span>
                      <span className="text-gray-500"> → {t.predicate_label} → </span>
                      <span>{t.object.slice(0, 50)}...</span>
                    </div>
                  ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
