import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Layout } from './components/Layout';
import { Dashboard } from './pages/Dashboard';
import { Search } from './pages/Search';
import { Ontology } from './pages/Ontology';
import { Settings } from './pages/Settings';
import { ErrorBoundary, PageErrorBoundary } from './components/ErrorBoundary';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

/**
 * Root error handler for logging errors to console or external service
 */
const handleError = (error: Error, errorInfo: React.ErrorInfo) => {
  // Log to console in development
  console.error('Application error:', error);
  console.error('Component stack:', errorInfo.componentStack);

  // TODO: Send to error tracking service (e.g., Sentry) in production
  // if (process.env.NODE_ENV === 'production') {
  //   errorTrackingService.captureException(error, { extra: errorInfo });
  // }
};

function App() {
  return (
    <ErrorBoundary onError={handleError}>
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <Routes>
            <Route path="/" element={<Layout />}>
              <Route
                index
                element={
                  <PageErrorBoundary>
                    <Dashboard />
                  </PageErrorBoundary>
                }
              />
              <Route
                path="search"
                element={
                  <PageErrorBoundary>
                    <Search />
                  </PageErrorBoundary>
                }
              />
              <Route
                path="ontology"
                element={
                  <PageErrorBoundary>
                    <Ontology />
                  </PageErrorBoundary>
                }
              />
              <Route
                path="settings"
                element={
                  <PageErrorBoundary>
                    <Settings />
                  </PageErrorBoundary>
                }
              />
            </Route>
          </Routes>
        </BrowserRouter>
      </QueryClientProvider>
    </ErrorBoundary>
  );
}

export default App;
