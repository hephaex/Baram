/**
 * Custom React Query hooks for API data fetching
 * Issue #19: Replace demo data with actual API calls
 */
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import * as api from '../api/client';
import type { SearchParams } from '../types';

// Query keys for cache management
export const queryKeys = {
  crawlStats: ['crawlStats'] as const,
  systemStatus: ['systemStatus'] as const,
  ontologyStats: ['ontologyStats'] as const,
  ontologyGraph: (entity?: string) => ['ontologyGraph', entity] as const,
  search: (params: SearchParams) => ['search', params] as const,
  triples: (query: string) => ['triples', query] as const,
};

// Dashboard hooks
export function useCrawlStats() {
  return useQuery({
    queryKey: queryKeys.crawlStats,
    queryFn: api.getCrawlStats,
    staleTime: 30 * 1000, // 30 seconds
    retry: 2,
  });
}

export function useSystemStatus() {
  return useQuery({
    queryKey: queryKeys.systemStatus,
    queryFn: api.getSystemStatus,
    staleTime: 10 * 1000, // 10 seconds
    retry: 2,
  });
}

// Ontology hooks
export function useOntologyStats() {
  return useQuery({
    queryKey: queryKeys.ontologyStats,
    queryFn: api.getOntologyStats,
    staleTime: 60 * 1000, // 1 minute
  });
}

export function useOntologyGraph(entity?: string) {
  return useQuery({
    queryKey: queryKeys.ontologyGraph(entity),
    queryFn: () => api.getOntologyGraph(entity),
    staleTime: 60 * 1000,
  });
}

export function useSearchTriples(query: string, enabled = true) {
  return useQuery({
    queryKey: queryKeys.triples(query),
    queryFn: () => api.searchTriples(query),
    enabled: enabled && query.length > 0,
    staleTime: 30 * 1000,
  });
}

// Search hook
export function useSearch(params: SearchParams, enabled = true) {
  return useQuery({
    queryKey: queryKeys.search(params),
    queryFn: () => api.searchArticles(params),
    enabled,
    staleTime: 30 * 1000,
  });
}

// Crawl control mutations
export function useStartCrawl() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (category?: string) => api.startCrawl(category),
    onSuccess: () => {
      // Invalidate stats after starting crawl
      queryClient.invalidateQueries({ queryKey: queryKeys.crawlStats });
      queryClient.invalidateQueries({ queryKey: queryKeys.systemStatus });
    },
  });
}

export function useStopCrawl() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: api.stopCrawl,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.crawlStats });
      queryClient.invalidateQueries({ queryKey: queryKeys.systemStatus });
    },
  });
}

// Refresh all dashboard data
export function useRefreshDashboard() {
  const queryClient = useQueryClient();

  return {
    refresh: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.crawlStats });
      queryClient.invalidateQueries({ queryKey: queryKeys.systemStatus });
      queryClient.invalidateQueries({ queryKey: queryKeys.ontologyStats });
    },
  };
}
