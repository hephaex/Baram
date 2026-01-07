/**
 * Search page with React Query integration
 * Issue #19: React Query + API Client integration
 * Issue #35: useMemo/useCallback performance optimization
 */
import { useState, useCallback, useMemo } from 'react';
import { Search as SearchIcon, Filter, Calendar, AlertCircle } from 'lucide-react';
import type { SearchParams } from '../types';
import { useSearch } from '../hooks/useApi';
import { LoadingFallback } from '../components/ErrorBoundary';

const categories = ['전체', 'IT', '경제', '정치', '사회', '문화', '세계', '스포츠'];

export function Search() {
  const [query, setQuery] = useState('');
  const [searchParams, setSearchParams] = useState<SearchParams>({
    query: '',
    category: undefined,
    date_from: undefined,
    date_to: undefined,
  });
  const [showFilters, setShowFilters] = useState(false);

  // Use React Query for search
  const {
    data: searchResults,
    isLoading,
    error,
    refetch,
  } = useSearch(searchParams, searchParams.query.length > 0);

  const results = searchResults?.articles ?? [];

  const handleSearch = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    setSearchParams((prev) => ({ ...prev, query }));
  }, [query]);

  const handleFilterChange = useCallback((key: keyof SearchParams, value: string | undefined) => {
    setSearchParams((prev) => ({
      ...prev,
      [key]: value === '전체' ? undefined : value,
    }));
  }, []);

  // Memoized highlight function
  const highlightText = useCallback((text: string, highlight: string) => {
    if (!highlight) return text;
    try {
      const escapedHighlight = highlight.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const parts = text.split(new RegExp(`(${escapedHighlight})`, 'gi'));
      return parts.map((part, i) =>
        part.toLowerCase() === highlight.toLowerCase() ? (
          <mark key={i} className="bg-yellow-200 px-0.5 rounded">
            {part}
          </mark>
        ) : (
          part
        )
      );
    } catch {
      return text;
    }
  }, []);

  // Memoized date formatter
  const formatDate = useMemo(() => {
    const formatter = new Intl.DateTimeFormat('ko-KR', {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
    return (dateStr: string) => {
      try {
        return formatter.format(new Date(dateStr));
      } catch {
        return dateStr;
      }
    };
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
            <label htmlFor="search-input" className="sr-only">
              기사 검색
            </label>
            <SearchIcon className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400" aria-hidden="true" />
            <input
              id="search-input"
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
            aria-label="검색 필터 토글"
            aria-expanded={showFilters}
            aria-controls="search-filters"
            className={`px-4 py-3 rounded-xl border transition focus:outline-none focus:ring-2 focus:ring-blue-500 ${
              showFilters
                ? 'bg-blue-50 border-blue-500 text-blue-600'
                : 'border-gray-200 text-gray-600 hover:bg-gray-50'
            }`}
          >
            <Filter className="w-5 h-5" aria-hidden="true" />
          </button>
          <button
            type="submit"
            disabled={isLoading}
            aria-label={isLoading ? '검색 진행 중' : '검색 실행'}
            className="px-6 py-3 bg-blue-600 text-white rounded-xl hover:bg-blue-700 disabled:opacity-50 transition focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
          >
            {isLoading ? '검색 중...' : '검색'}
          </button>
        </div>

        {/* Filters */}
        {showFilters && (
          <div id="search-filters" className="mt-4 p-4 bg-gray-50 rounded-xl" role="region" aria-label="검색 필터">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              {/* Category */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  카테고리
                </label>
                <select
                  value={searchParams.category || '전체'}
                  onChange={(e) => handleFilterChange('category', e.target.value)}
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
                  value={searchParams.date_from || ''}
                  onChange={(e) => handleFilterChange('date_from', e.target.value || undefined)}
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
                  value={searchParams.date_to || ''}
                  onChange={(e) => handleFilterChange('date_to', e.target.value || undefined)}
                  className="w-full px-3 py-2 rounded-lg border border-gray-200 focus:border-blue-500 outline-none"
                />
              </div>
            </div>
          </div>
        )}
      </form>

      {/* Error State */}
      {error && (
        <div className="bg-red-50 border border-red-200 rounded-lg p-6 text-center mb-6">
          <AlertCircle className="w-12 h-12 text-red-500 mx-auto mb-4" />
          <h3 className="text-lg font-semibold text-red-800 mb-2">검색 오류</h3>
          <p className="text-red-600 mb-4">{(error as Error).message}</p>
          <button
            onClick={() => refetch()}
            className="px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700"
          >
            다시 시도
          </button>
        </div>
      )}

      {/* Loading State */}
      {isLoading && <LoadingFallback />}

      {/* Results */}
      {!isLoading && !error && (
        <div className="space-y-4">
          <p className="text-sm text-gray-500">
            {searchParams.query
              ? `"${searchParams.query}"에 대한 ${results.length}개의 결과`
              : '검색어를 입력하세요'}
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
                      {highlightText(article.title, searchParams.query)}
                    </a>
                  </h3>

                  <p className="text-gray-600 line-clamp-2 mb-3">
                    {highlightText(article.content.slice(0, 200), searchParams.query)}...
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

          {searchParams.query && results.length === 0 && (
            <div className="text-center py-12">
              <SearchIcon className="w-12 h-12 text-gray-300 mx-auto mb-4" />
              <p className="text-gray-500">검색 결과가 없습니다</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
