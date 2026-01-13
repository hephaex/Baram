/**
 * Main App with code splitting and error boundaries
 * Issue #26: Code Splitting/Lazy Loading
 * Issue #36: React Error Boundaries
 */
import { lazy, Suspense } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Layout } from './components/Layout';
import { ErrorBoundary, PageLoadingFallback } from './components/ErrorBoundary';
import './i18n/config';

// Lazy load pages for code splitting
const Dashboard = lazy(() =>
  import('./pages/Dashboard').then((module) => ({ default: module.Dashboard }))
);
const Search = lazy(() =>
  import('./pages/Search').then((module) => ({ default: module.Search }))
);
const Ontology = lazy(() =>
  import('./pages/Ontology').then((module) => ({ default: module.Ontology }))
);
const Settings = lazy(() =>
  import('./pages/Settings').then((module) => ({ default: module.Settings }))
);

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
      staleTime: 30 * 1000, // 30 seconds default stale time
    },
  },
});

function App() {
  return (
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Suspense fallback={<PageLoadingFallback />}>
            <Routes>
              <Route path="/" element={<Layout />}>
                <Route
                  index
                  element={
                    <ErrorBoundary>
                      <Dashboard />
                    </ErrorBoundary>
                  }
                />
                <Route
                  path="search"
                  element={
                    <ErrorBoundary>
                      <Search />
                    </ErrorBoundary>
                  }
                />
                <Route
                  path="ontology"
                  element={
                    <ErrorBoundary>
                      <Ontology />
                    </ErrorBoundary>
                  }
                />
                <Route
                  path="settings"
                  element={
                    <ErrorBoundary>
                      <Settings />
                    </ErrorBoundary>
                  }
                />
              </Route>
            </Routes>
          </Suspense>
        </BrowserRouter>
      </QueryClientProvider>
    </ErrorBoundary>
  );
}

export default App;
