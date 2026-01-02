import { useState, useCallback } from 'react';
import { Search as SearchIcon, Filter, Calendar } from 'lucide-react';
import type { Article, SearchParams } from '../types';

// Demo data
const demoArticles: Article[] = [
  {
    id: '001_0015822338',
    oid: '001',
    aid: '0015822338',
    title: '소프트뱅크, 오픈AI에 410억달러 투자 완료...11% 지분 확보',
    content: '일본 소프트뱅크가 미국 인공지능(AI) 기업 오픈AI에 410억 달러를 투자해 11%의 지분을 확보했다...',
    url: 'https://news.naver.com/article/001/0015822338',
    category: 'IT',
    publisher: '연합뉴스',
    author: '홍길동',
    published_at: '2025-12-31T14:30:00Z',
    crawled_at: '2025-12-31T15:00:00Z',
  },
  {
    id: '001_0015822411',
    oid: '001',
    aid: '0015822411',
    title: '흡연자 날숨에 노출도 간접흡연...실내 흡연구역 없애야',
    content: '흡연자의 날숨만으로도 간접흡연 피해가 발생할 수 있어 실내 흡연구역을 없애야 한다는 연구 결과가 나왔다...',
    url: 'https://news.naver.com/article/001/0015822411',
    category: '사회',
    publisher: '연합뉴스',
    published_at: '2025-12-31T13:00:00Z',
    crawled_at: '2025-12-31T14:00:00Z',
  },
  {
    id: '001_0015822578',
    oid: '001',
    aid: '0015822578',
    title: '대만 TSMC 2나노 제품 양산 개시...성능 획기적 발전',
    content: '대만 TSMC가 2나노 공정 제품의 양산을 시작했다. 성능이 크게 향상될 것으로 기대된다...',
    url: 'https://news.naver.com/article/001/0015822578',
    category: 'IT',
    publisher: '연합뉴스',
    published_at: '2025-12-31T12:00:00Z',
    crawled_at: '2025-12-31T13:00:00Z',
  },
];

const categories = ['전체', 'IT', '경제', '정치', '사회', '문화', '세계', '스포츠'];

export function Search() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<Article[]>(demoArticles);
  const [filters, setFilters] = useState<SearchParams>({
    query: '',
    category: undefined,
    date_from: undefined,
    date_to: undefined,
  });
  const [showFilters, setShowFilters] = useState(false);
  const [isSearching, setIsSearching] = useState(false);

  // Memoize search handler
  const handleSearch = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      setIsSearching(true);

      // Simulate search
      await new Promise((resolve) => setTimeout(resolve, 500));

      // Filter demo data
      const filtered = demoArticles.filter((article) => {
        const matchesQuery =
          !query ||
          article.title.toLowerCase().includes(query.toLowerCase()) ||
          article.content.toLowerCase().includes(query.toLowerCase());

        const matchesCategory =
          !filters.category ||
          filters.category === '전체' ||
          article.category === filters.category;

        return matchesQuery && matchesCategory;
      });

      setResults(filtered);
      setIsSearching(false);
    },
    [query, filters.category]
  );

  // Memoize text highlighting function
  const highlightText = useCallback((text: string, highlight: string) => {
    if (!highlight) return text;
    const parts = text.split(new RegExp(`(${highlight})`, 'gi'));
    return parts.map((part, i) =>
      part.toLowerCase() === highlight.toLowerCase() ? (
        <mark key={i} className="bg-yellow-200 px-0.5 rounded">
          {part}
        </mark>
      ) : (
        part
      )
    );
  }, []);

  // Memoize date formatter
  const formatDate = useCallback((dateStr: string) => {
    return new Date(dateStr).toLocaleString('ko-KR', {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }, []);

  return (
    <div className="p-6">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-gray-900">Search</h1>
        <p className="text-gray-500">수집된 기사를 검색합니다</p>
      </div>

      {/* Search Form */}
      <form onSubmit={handleSearch} className="mb-6">
        <div className="flex gap-2">
          <div className="flex-1 relative">
            <SearchIcon className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="검색어를 입력하세요..."
              className="w-full pl-12 pr-4 py-3 rounded-xl border border-gray-200 focus:border-blue-500 focus:ring-2 focus:ring-blue-200 outline-none transition"
            />
          </div>
          <button
            type="button"
            onClick={() => setShowFilters(!showFilters)}
            className={`px-4 py-3 rounded-xl border transition ${
              showFilters
                ? 'bg-blue-50 border-blue-500 text-blue-600'
                : 'border-gray-200 text-gray-600 hover:bg-gray-50'
            }`}
          >
            <Filter className="w-5 h-5" />
          </button>
          <button
            type="submit"
            disabled={isSearching}
            className="px-6 py-3 bg-blue-600 text-white rounded-xl hover:bg-blue-700 disabled:opacity-50 transition"
          >
            {isSearching ? '검색 중...' : '검색'}
          </button>
        </div>

        {/* Filters */}
        {showFilters && (
          <div className="mt-4 p-4 bg-gray-50 rounded-xl">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {/* Category */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  카테고리
                </label>
                <select
                  value={filters.category || '전체'}
                  onChange={(e) =>
                    setFilters({ ...filters, category: e.target.value })
                  }
                  className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
                >
                  {categories.map((cat) => (
                    <option key={cat} value={cat}>
                      {cat}
                    </option>
                  ))}
                </select>
              </div>

              {/* Date from */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  시작일
                </label>
                <input
                  type="date"
                  value={filters.date_from || ''}
                  onChange={(e) =>
                    setFilters({ ...filters, date_from: e.target.value })
                  }
                  className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
                />
              </div>

              {/* Date to */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  종료일
                </label>
                <input
                  type="date"
                  value={filters.date_to || ''}
                  onChange={(e) =>
                    setFilters({ ...filters, date_to: e.target.value })
                  }
                  className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
                />
              </div>
            </div>
          </div>
        )}
      </form>

      {/* Results */}
      <div className="space-y-4">
        <p className="text-sm text-gray-500">
          {results.length}개의 결과를 찾았습니다
        </p>

        {results.map((article) => (
          <article
            key={article.id}
            className="bg-white rounded-xl shadow-sm p-6 hover:shadow-md transition"
          >
            <div className="flex items-start justify-between gap-4">
              <div className="flex-1">
                <div className="flex items-center gap-2 mb-2">
                  <span className="px-2 py-0.5 bg-blue-100 text-blue-700 text-xs font-medium rounded">
                    {article.category}
                  </span>
                  <span className="text-sm text-gray-500">
                    {article.publisher}
                  </span>
                </div>

                <h3 className="text-lg font-semibold text-gray-900 mb-2">
                  <a
                    href={article.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="hover:text-blue-600"
                  >
                    {highlightText(article.title, query)}
                  </a>
                </h3>

                <p className="text-gray-600 line-clamp-2 mb-3">
                  {highlightText(article.content.slice(0, 200), query)}...
                </p>

                <div className="flex items-center gap-4 text-sm text-gray-500">
                  <span className="flex items-center gap-1">
                    <Calendar className="w-4 h-4" />
                    {formatDate(article.published_at)}
                  </span>
                  {article.author && <span>by {article.author}</span>}
                </div>
              </div>
            </div>
          </article>
        ))}

        {results.length === 0 && (
          <div className="text-center py-12">
            <SearchIcon className="w-12 h-12 text-gray-300 mx-auto mb-4" />
            <p className="text-gray-500">검색 결과가 없습니다</p>
          </div>
        )}
      </div>
    </div>
  );
}
