import { useState } from 'react';
import {
  Newspaper,
  TrendingUp,
  Clock,
  HardDrive,
  Activity,
  RefreshCw,
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

// Demo data (replace with API calls)
const demoStats: CrawlStats = {
  total_articles: 31247,
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

const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6'];

export function Dashboard() {
  const [stats] = useState<CrawlStats>(demoStats);
  const [status] = useState<SystemStatus>(demoStatus);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const categoryData = Object.entries(stats.categories).map(([name, value]) => ({
    name,
    value,
  }));

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
          value={stats.total_articles}
          icon={Newspaper}
        />
        <StatCard
          title="오늘 수집"
          value={stats.today_articles}
          icon={TrendingUp}
          change="+12% vs yesterday"
          changeType="positive"
        />
        <StatCard
          title="시스템 가동"
          value={formatUptime(status.uptime)}
          icon={Clock}
        />
        <StatCard
          title="디스크 사용량"
          value={`${status.disk_usage}%`}
          icon={HardDrive}
          changeType={status.disk_usage > 80 ? 'negative' : 'neutral'}
        />
      </div>

      {/* Charts */}
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

        {/* Category distribution */}
        <div className="bg-white rounded-xl shadow-sm p-6">
          <h3 className="text-lg font-semibold mb-4">카테고리별 분포</h3>
          <ResponsiveContainer width="100%" height={250}>
            <PieChart>
              <Pie
                data={categoryData}
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
                {categoryData.map((_, index) => (
                  <Cell key={index} fill={COLORS[index % COLORS.length]} />
                ))}
              </Pie>
              <Tooltip />
            </PieChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Hourly chart */}
      <div className="bg-white rounded-xl shadow-sm p-6 mb-6">
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

      {/* System Status */}
      <div className="bg-white rounded-xl shadow-sm p-6">
        <h3 className="text-lg font-semibold mb-4">시스템 상태</h3>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
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
        </div>
      </div>
    </div>
  );
}
