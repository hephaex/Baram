/**
 * Layout component with accessibility improvements
 * Issue #21: WCAG Accessibility Compliance
 */
import { Outlet, NavLink } from 'react-router-dom';
import {
  LayoutDashboard,
  Search,
  Network,
  Settings,
  Database,
} from 'lucide-react';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/search', icon: Search, label: 'Search' },
  { to: '/ontology', icon: Network, label: 'Ontology' },
  { to: '/settings', icon: Settings, label: 'Settings' },
];

export function Layout() {
  return (
    <div className="flex h-screen bg-gray-100">
      {/* Skip to main content link for keyboard users */}
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-blue-600 focus:text-white focus:rounded-lg focus:outline-none focus:ring-2 focus:ring-white"
      >
        메인 콘텐츠로 건너뛰기
      </a>

      {/* Sidebar */}
      <aside className="w-64 bg-gray-900 text-white" role="complementary" aria-label="사이드바">
        <div className="p-4 border-b border-gray-800">
          <div className="flex items-center gap-2">
            <Database className="w-8 h-8 text-blue-400" aria-hidden="true" />
            <div>
              <h1 className="text-xl font-bold">Baram</h1>
              <p className="text-xs text-gray-400">News Crawler Dashboard</p>
            </div>
          </div>
        </div>

        <nav className="p-4" role="navigation" aria-label="메인 네비게이션">
          <ul className="space-y-2" role="list">
            {navItems.map(({ to, icon: Icon, label }) => (
              <li key={to} role="listitem">
                <NavLink
                  to={to}
                  aria-label={label}
                  className={({ isActive }) =>
                    `flex items-center gap-3 px-4 py-2 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-blue-400 ${
                      isActive
                        ? 'bg-blue-600 text-white'
                        : 'text-gray-300 hover:bg-gray-800'
                    }`
                  }
                >
                  {({ isActive }) => (
                    <>
                      <Icon className="w-5 h-5" aria-hidden="true" />
                      <span aria-current={isActive ? 'page' : undefined}>{label}</span>
                    </>
                  )}
                </NavLink>
              </li>
            ))}
          </ul>
        </nav>
      </aside>

      {/* Main content */}
      <main id="main-content" className="flex-1 overflow-auto" role="main" aria-label="메인 콘텐츠" tabIndex={-1}>
        <Outlet />
      </main>
    </div>
  );
}
