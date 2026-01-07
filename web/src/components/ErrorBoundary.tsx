/**
 * React Error Boundary component
 * Issue #36: Graceful error handling for React components
 */
import React, { Component, type ReactNode } from 'react';

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo): void {
    console.error('ErrorBoundary caught an error:', error, errorInfo);
    this.props.onError?.(error, errorInfo);
  }

  handleRetry = (): void => {
    this.setState({ hasError: false, error: null });
  };

  render(): ReactNode {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div className="min-h-[200px] flex items-center justify-center">
          <div className="text-center p-6 bg-red-50 dark:bg-red-900/20 rounded-lg max-w-md">
            <div className="text-red-600 dark:text-red-400 mb-4">
              <svg
                className="w-12 h-12 mx-auto"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                />
              </svg>
            </div>
            <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-2">
              오류가 발생했습니다
            </h3>
            <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">
              {this.state.error?.message || '예상치 못한 오류가 발생했습니다.'}
            </p>
            <button
              onClick={this.handleRetry}
              className="px-4 py-2 bg-red-600 text-white rounded-md hover:bg-red-700 transition-colors"
            >
              다시 시도
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

// Query error fallback for React Query errors
interface QueryErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
}

export function QueryErrorFallback({ error, resetErrorBoundary }: QueryErrorFallbackProps): React.ReactElement {
  return (
    <div className="min-h-[200px] flex items-center justify-center">
      <div className="text-center p-6 bg-yellow-50 dark:bg-yellow-900/20 rounded-lg max-w-md">
        <div className="text-yellow-600 dark:text-yellow-400 mb-4">
          <svg
            className="w-12 h-12 mx-auto"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M20.618 5.984A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016zM12 9v2m0 4h.01"
            />
          </svg>
        </div>
        <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-2">
          데이터 로딩 실패
        </h3>
        <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">
          {error.message || '서버에서 데이터를 가져오는데 실패했습니다.'}
        </p>
        <button
          onClick={resetErrorBoundary}
          className="px-4 py-2 bg-yellow-600 text-white rounded-md hover:bg-yellow-700 transition-colors"
        >
          다시 시도
        </button>
      </div>
    </div>
  );
}

// Loading fallback component
export function LoadingFallback(): React.ReactElement {
  return (
    <div className="min-h-[200px] flex items-center justify-center">
      <div className="text-center">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4" />
        <p className="text-gray-600 dark:text-gray-400">로딩 중...</p>
      </div>
    </div>
  );
}

// Page-level loading fallback
export function PageLoadingFallback(): React.ReactElement {
  return (
    <div className="min-h-screen flex items-center justify-center">
      <div className="text-center">
        <div className="animate-spin rounded-full h-16 w-16 border-b-2 border-blue-600 mx-auto mb-4" />
        <p className="text-lg text-gray-600 dark:text-gray-400">페이지 로딩 중...</p>
      </div>
    </div>
  );
}
