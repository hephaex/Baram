import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  getCrawlStats,
  getSystemStatus,
  searchArticles,
  getOntologyStats,
  startCrawl,
  stopCrawl,
} from './client';

// Helper to create mock fetch response
function mockFetchResponse<T>(data: T, ok = true, status = 200) {
  return vi.fn().mockResolvedValue({
    ok,
    status,
    statusText: ok ? 'OK' : 'Error',
    json: () => Promise.resolve(data),
  });
}

describe('API Client', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  describe('getCrawlStats', () => {
    it('should fetch crawl stats successfully', async () => {
      const mockStats = {
        total_articles: 1000,
        today_articles: 50,
        categories: { politics: 100, economy: 200 },
      };
      global.fetch = mockFetchResponse(mockStats);

      const result = await getCrawlStats();

      expect(result).toEqual(mockStats);
      expect(fetch).toHaveBeenCalledWith('/api/stats', expect.any(Object));
    });

    it('should throw error on API failure', async () => {
      global.fetch = mockFetchResponse({}, false, 500);

      await expect(getCrawlStats()).rejects.toThrow('API error: 500');
    });
  });

  describe('getSystemStatus', () => {
    it('should fetch system status successfully', async () => {
      const mockStatus = {
        crawler_running: true,
        last_crawl: '2024-01-01T00:00:00Z',
        db_size: '100MB',
      };
      global.fetch = mockFetchResponse(mockStatus);

      const result = await getSystemStatus();

      expect(result).toEqual(mockStatus);
      expect(fetch).toHaveBeenCalledWith('/api/status', expect.any(Object));
    });
  });

  describe('searchArticles', () => {
    it('should search with query parameter', async () => {
      const mockResult = { articles: [], total: 0, page: 1 };
      global.fetch = mockFetchResponse(mockResult);

      await searchArticles({ query: 'test' });

      expect(fetch).toHaveBeenCalledWith(
        expect.stringContaining('q=test'),
        expect.any(Object)
      );
    });

    it('should include all search parameters', async () => {
      const mockResult = { articles: [], total: 0, page: 1 };
      global.fetch = mockFetchResponse(mockResult);

      await searchArticles({
        query: 'news',
        category: 'politics',
        publisher: 'naver',
        date_from: '2024-01-01',
        date_to: '2024-01-31',
        page: 2,
        limit: 20,
      });

      const callUrl = (fetch as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
      expect(callUrl).toContain('q=news');
      expect(callUrl).toContain('category=politics');
      expect(callUrl).toContain('publisher=naver');
      expect(callUrl).toContain('from=2024-01-01');
      expect(callUrl).toContain('to=2024-01-31');
      expect(callUrl).toContain('page=2');
      expect(callUrl).toContain('limit=20');
    });
  });

  describe('getOntologyStats', () => {
    it('should fetch ontology stats', async () => {
      const mockStats = {
        total_entities: 500,
        total_relations: 1000,
        entity_types: { person: 100, organization: 200 },
      };
      global.fetch = mockFetchResponse(mockStats);

      const result = await getOntologyStats();

      expect(result).toEqual(mockStats);
      expect(fetch).toHaveBeenCalledWith('/api/ontology/stats', expect.any(Object));
    });
  });

  describe('startCrawl', () => {
    it('should start crawl without category', async () => {
      const mockResponse = { message: 'Crawl started' };
      global.fetch = mockFetchResponse(mockResponse);

      const result = await startCrawl();

      expect(result).toEqual(mockResponse);
      expect(fetch).toHaveBeenCalledWith('/api/crawl/start', {
        method: 'POST',
        body: JSON.stringify({ category: undefined }),
        headers: { 'Content-Type': 'application/json' },
      });
    });

    it('should start crawl with specific category', async () => {
      const mockResponse = { message: 'Crawl started for politics' };
      global.fetch = mockFetchResponse(mockResponse);

      await startCrawl('politics');

      expect(fetch).toHaveBeenCalledWith('/api/crawl/start', {
        method: 'POST',
        body: JSON.stringify({ category: 'politics' }),
        headers: { 'Content-Type': 'application/json' },
      });
    });
  });

  describe('stopCrawl', () => {
    it('should stop crawl', async () => {
      const mockResponse = { message: 'Crawl stopped' };
      global.fetch = mockFetchResponse(mockResponse);

      const result = await stopCrawl();

      expect(result).toEqual(mockResponse);
      expect(fetch).toHaveBeenCalledWith('/api/crawl/stop', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
      });
    });
  });
});
