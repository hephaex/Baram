import type {
  CrawlStats,
  SystemStatus,
  SearchParams,
  SearchResult,
  OntologyStats,
  GraphData,
  Triple
} from '../types';

const API_BASE = '/api';

async function fetchJson<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });

  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }

  return response.json();
}

// Dashboard API
export async function getCrawlStats(): Promise<CrawlStats> {
  return fetchJson<CrawlStats>(`${API_BASE}/stats`);
}

export async function getSystemStatus(): Promise<SystemStatus> {
  return fetchJson<SystemStatus>(`${API_BASE}/status`);
}

// Search API
export async function searchArticles(params: SearchParams): Promise<SearchResult> {
  const searchParams = new URLSearchParams();

  if (params.query) searchParams.set('q', params.query);
  if (params.category) searchParams.set('category', params.category);
  if (params.publisher) searchParams.set('publisher', params.publisher);
  if (params.date_from) searchParams.set('from', params.date_from);
  if (params.date_to) searchParams.set('to', params.date_to);
  if (params.page) searchParams.set('page', String(params.page));
  if (params.limit) searchParams.set('limit', String(params.limit));

  return fetchJson<SearchResult>(`${API_BASE}/search?${searchParams}`);
}

// Ontology API
export async function getOntologyStats(): Promise<OntologyStats> {
  return fetchJson<OntologyStats>(`${API_BASE}/ontology/stats`);
}

export async function getOntologyGraph(entity?: string): Promise<GraphData> {
  const params = entity ? `?entity=${encodeURIComponent(entity)}` : '';
  return fetchJson<GraphData>(`${API_BASE}/ontology/graph${params}`);
}

export async function searchTriples(query: string): Promise<Triple[]> {
  return fetchJson<Triple[]>(`${API_BASE}/ontology/search?q=${encodeURIComponent(query)}`);
}

// Crawl control API
export async function startCrawl(category?: string): Promise<{ message: string }> {
  return fetchJson(`${API_BASE}/crawl/start`, {
    method: 'POST',
    body: JSON.stringify({ category }),
  });
}

export async function stopCrawl(): Promise<{ message: string }> {
  return fetchJson(`${API_BASE}/crawl/stop`, {
    method: 'POST',
  });
}
