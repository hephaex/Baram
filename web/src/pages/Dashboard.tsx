import { useState, useEffect } from 'react';
import {
  Newspaper,
  TrendingUp,
  Clock,
  HardDrive,
  Activity,
  RefreshCw,
  Network,
  Users,
  Building2,
  MapPin,
} from 'lucide-react';
import {
  AreaChart,
  Area,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  PieChart,
  Pie,
  Cell,
} from 'recharts';
import { StatCard } from '../components/StatCard';
import type { CrawlStats, SystemStatus } from '../types';

interface OntologyStats {
  total_articles: number;
  total_entities: number;
  total_triples: number;
  entity_types: Record<string, number>;
  relation_types: Record<string, number>;
}

// Demo data (replace with API calls)
const demoStats: CrawlStats = {
  total_articles: 33934,
  today_articles: 342,
  categories: {
    'IT': 5234,
    '경제': 8921,
    '정치': 6543,
    '사회': 7234,
    '문화': 3315,
  },
  publishers: {
    '연합뉴스': 4521,
    'KBS': 3211,
    'MBC': 2890,
    'SBS': 2654,
    '조선일보': 2341,
  },
  hourly_counts: Array.from({ length: 24 }, (_, i) => ({
    hour: `${i}시`,
    count: Math.floor(Math.random() * 50) + 10,
  })),
  daily_counts: Array.from({ length: 7 }, (_, i) => {
    const date = new Date();
    date.setDate(date.getDate() - (6 - i));
    return {
      date: date.toLocaleDateString('ko-KR', { month: 'short', day: 'numeric' }),
      count: Math.floor(Math.random() * 500) + 200,
    };
  }),
};

const demoStatus: SystemStatus = {
  database: 'healthy',
  llm: 'healthy',
  disk_usage: 45.2,
  uptime: 86400 * 7,
};

const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6', '#ec4899', '#14b8a6'];

const entityTypeLabels: Record<string, string> = {
  '기관': 'Organization',
  '장소': 'Location',
  '인물': 'Person',
  '비율': 'Percentage',
  '금액': 'Money',
};

export function Dashboard() {
  const [stats] = useState<CrawlStats>(demoStats);
  const [status] = useState<SystemStatus>(demoStatus);
  const [ontologyStats, setOntologyStats] = useState<OntologyStats | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);

  // Load ontology stats
  useEffect(() => {
    const loadOntologyStats = async () => {
      try {
        const response = await fetch('/data/ontology-summary.json');
        if (response.ok) {
          const data = await response.json();
          setOntologyStats(data.stats);
        }
      } catch (err) {
        console.error('Failed to load ontology stats:', err);
      }
    };

    loadOntologyStats();
  }, []);

  // Category data available but not displayed in current layout
  const _categoryData = Object.entries(stats.categories).map(([name, value]) => ({
    name,
    value,
  }));
  void _categoryData; // Suppress unused variable warning

  const entityTypeData = ontologyStats
    ? Object.entries(ontologyStats.entity_types)
        .sort(([, a], [, b]) => b - a)
        .slice(0, 5)
        .map(([name, value]) => ({
          name: entityTypeLabels[name] || name,
          value,
        }))
    : [];

  const relationTypeData = ontologyStats
    ? Object.entries(ontologyStats.relation_types)
        .sort(([, a], [, b]) => b - a)
        .map(([name, value]) => ({
          name,
          value,
        }))
    : [];

  const handleRefresh = async () => {
    setIsRefreshing(true);
    // Simulate API call
    await new Promise((resolve) => setTimeout(resolve, 1000));
    setIsRefreshing(false);
  };

  const formatUptime = (seconds: number) => {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    return `${days}일 ${hours}시간`;
  };

  const formatNumber = (num: number) => {
    if (num >= 1000000) {
      return (num / 1000000).toFixed(1) + 'M';
    }
    if (num >= 1000) {
      return (num / 1000).toFixed(1) + 'K';
    }
    return num.toString();
  };

  return (
    <div className="p-6">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-gray-900">Dashboard</h1>
          <p className="text-gray-500">크롤링 현황 및 시스템 상태</p>
        </div>
        <button
          onClick={handleRefresh}
          disabled={isRefreshing}
          className="flex items-center gap-2 px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50"
        >
          <RefreshCw className={`w-4 h-4 ${isRefreshing ? 'animate-spin' : ''}`} />
          새로고침
        </button>
      </div>

      {/* Stats Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        <StatCard
          title="전체 기사"
          value={ontologyStats?.total_articles || stats.total_articles}
          icon={Newspaper}
        />
        <StatCard
          title="추출된 엔티티"
          value={formatNumber(ontologyStats?.total_entities || 0)}
          icon={Users}
          change={ontologyStats ? `${Object.keys(ontologyStats.entity_types).length} types` : undefined}
          changeType="neutral"
        />
        <StatCard
          title="추출된 관계"
          value={formatNumber(ontologyStats?.total_triples || 0)}
          icon={Network}
          change={ontologyStats ? `${Object.keys(ontologyStats.relation_types).length} types` : undefined}
          changeType="neutral"
        />
        <StatCard
          title="디스크 사용량"
          value={`${status.disk_usage}%`}
          icon={HardDrive}
          changeType={status.disk_usage > 80 ? 'negative' : 'neutral'}
        />
      </div>

      {/* Charts Row 1 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
        {/* Daily trend */}
        <div className="bg-white rounded-xl shadow-sm p-6">
          <h3 className="text-lg font-semibold mb-4">일별 수집 추이</h3>
          <ResponsiveContainer width="100%" height={250}>
            <AreaChart data={stats.daily_counts}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="date" />
              <YAxis />
              <Tooltip />
              <Area
                type="monotone"
                dataKey="count"
                stroke="#3b82f6"
                fill="#93c5fd"
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>

        {/* Entity Type Distribution */}
        <div className="bg-white rounded-xl shadow-sm p-6">
          <h3 className="text-lg font-semibold mb-4">엔티티 타입별 분포</h3>
          {entityTypeData.length > 0 ? (
            <ResponsiveContainer width="100%" height={250}>
              <PieChart>
                <Pie
                  data={entityTypeData}
                  cx="50%"
                  cy="50%"
                  innerRadius={60}
                  outerRadius={100}
                  paddingAngle={2}
                  dataKey="value"
                  label={({ name, percent }) =>
                    `${name ?? ''} ${((percent ?? 0) * 100).toFixed(0)}%`
                  }
                >
                  {entityTypeData.map((_, index) => (
                    <Cell key={index} fill={COLORS[index % COLORS.length]} />
                  ))}
                </Pie>
                <Tooltip formatter={(value) => (value as number).toLocaleString()} />
              </PieChart>
            </ResponsiveContainer>
          ) : (
            <div className="h-[250px] flex items-center justify-center text-gray-400">
              데이터 로딩 중...
            </div>
          )}
        </div>
      </div>

      {/* Charts Row 2 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
        {/* Relation Type Distribution */}
        <div className="bg-white rounded-xl shadow-sm p-6">
          <h3 className="text-lg font-semibold mb-4">관계 타입별 분포</h3>
          {relationTypeData.length > 0 ? (
            <ResponsiveContainer width="100%" height={200}>
              <BarChart data={relationTypeData} layout="vertical">
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis type="number" />
                <YAxis dataKey="name" type="category" width={60} />
                <Tooltip formatter={(value) => (value as number).toLocaleString()} />
                <Bar dataKey="value" fill="#10b981" radius={[0, 4, 4, 0]} />
              </BarChart>
            </ResponsiveContainer>
          ) : (
            <div className="h-[200px] flex items-center justify-center text-gray-400">
              데이터 로딩 중...
            </div>
          )}
        </div>

        {/* Hourly chart */}
        <div className="bg-white rounded-xl shadow-sm p-6">
          <h3 className="text-lg font-semibold mb-4">시간대별 수집량</h3>
          <ResponsiveContainer width="100%" height={200}>
            <BarChart data={stats.hourly_counts}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="hour" />
              <YAxis />
              <Tooltip />
              <Bar dataKey="count" fill="#3b82f6" radius={[4, 4, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Ontology Summary Cards */}
      {ontologyStats && (
        <div className="bg-white rounded-xl shadow-sm p-6 mb-6">
          <h3 className="text-lg font-semibold mb-4">온톨로지 추출 요약</h3>
          <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
            <div className="flex items-center gap-3 p-4 bg-blue-50 rounded-lg">
              <Users className="w-8 h-8 text-blue-500" />
              <div>
                <p className="text-2xl font-bold text-blue-600">
                  {formatNumber(ontologyStats.entity_types['인물'] || 0)}
                </p>
                <p className="text-sm text-gray-500">인물</p>
              </div>
            </div>
            <div className="flex items-center gap-3 p-4 bg-green-50 rounded-lg">
              <Building2 className="w-8 h-8 text-green-500" />
              <div>
                <p className="text-2xl font-bold text-green-600">
                  {formatNumber(ontologyStats.entity_types['기관'] || 0)}
                </p>
                <p className="text-sm text-gray-500">기관</p>
              </div>
            </div>
            <div className="flex items-center gap-3 p-4 bg-yellow-50 rounded-lg">
              <MapPin className="w-8 h-8 text-yellow-500" />
              <div>
                <p className="text-2xl font-bold text-yellow-600">
                  {formatNumber(ontologyStats.entity_types['장소'] || 0)}
                </p>
                <p className="text-sm text-gray-500">장소</p>
              </div>
            </div>
            <div className="flex items-center gap-3 p-4 bg-purple-50 rounded-lg">
              <Network className="w-8 h-8 text-purple-500" />
              <div>
                <p className="text-2xl font-bold text-purple-600">
                  {formatNumber(ontologyStats.relation_types['소속'] || 0)}
                </p>
                <p className="text-sm text-gray-500">소속 관계</p>
              </div>
            </div>
            <div className="flex items-center gap-3 p-4 bg-pink-50 rounded-lg">
              <Activity className="w-8 h-8 text-pink-500" />
              <div>
                <p className="text-2xl font-bold text-pink-600">
                  {formatNumber(ontologyStats.relation_types['대표'] || 0)}
                </p>
                <p className="text-sm text-gray-500">대표 관계</p>
              </div>
            </div>
            <div className="flex items-center gap-3 p-4 bg-cyan-50 rounded-lg">
              <TrendingUp className="w-8 h-8 text-cyan-500" />
              <div>
                <p className="text-2xl font-bold text-cyan-600">
                  {formatNumber(ontologyStats.relation_types['투자'] || 0)}
                </p>
                <p className="text-sm text-gray-500">투자 관계</p>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* System Status */}
      <div className="bg-white rounded-xl shadow-sm p-6">
        <h3 className="text-lg font-semibold mb-4">시스템 상태</h3>
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          <div className="flex items-center gap-3 p-4 bg-gray-50 rounded-lg">
            <div
              className={`w-3 h-3 rounded-full ${
                status.database === 'healthy' ? 'bg-green-500' : 'bg-red-500'
              }`}
            />
            <div>
              <p className="font-medium">Database</p>
              <p className="text-sm text-gray-500">SQLite</p>
            </div>
          </div>
          <div className="flex items-center gap-3 p-4 bg-gray-50 rounded-lg">
            <div
              className={`w-3 h-3 rounded-full ${
                status.llm === 'healthy'
                  ? 'bg-green-500'
                  : status.llm === 'unavailable'
                  ? 'bg-yellow-500'
                  : 'bg-red-500'
              }`}
            />
            <div>
              <p className="font-medium">LLM Server</p>
              <p className="text-sm text-gray-500">vLLM / Ollama</p>
            </div>
          </div>
          <div className="flex items-center gap-3 p-4 bg-gray-50 rounded-lg">
            <Activity className="w-5 h-5 text-green-500" />
            <div>
              <p className="font-medium">Crawler</p>
              <p className="text-sm text-gray-500">Running</p>
            </div>
          </div>
          <div className="flex items-center gap-3 p-4 bg-gray-50 rounded-lg">
            <Clock className="w-5 h-5 text-blue-500" />
            <div>
              <p className="font-medium">Uptime</p>
              <p className="text-sm text-gray-500">{formatUptime(status.uptime)}</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
