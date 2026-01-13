/**
 * Layout component with accessibility improvements
 * Issue #21: WCAG Accessibility Compliance
 */
import { Outlet, NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import {
  LayoutDashboard,
  Search,
  Network,
  Settings,
  Database,
} from 'lucide-react';
import { LanguageSwitcher } from '../i18n/LanguageSwitcher';

export function Layout() {
  const { t } = useTranslation('common');

  const navItems = [
    { to: '/', icon: LayoutDashboard, label: t('nav.dashboard') },
    { to: '/search', icon: Search, label: t('nav.search') },
    { to: '/ontology', icon: Network, label: t('nav.ontology') },
    { to: '/settings', icon: Settings, label: t('nav.settings') },
  ];

  return (
    <div className="flex h-screen bg-gray-100">
      {/* Skip to main content link for keyboard users */}
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-blue-600 focus:text-white focus:rounded-lg focus:outline-none focus:ring-2 focus:ring-white"
      >
        {t('nav.skipToMain')}
      </a>

      {/* Sidebar */}
      <aside className="w-64 bg-gray-900 text-white flex flex-col" role="complementary" aria-label={t('aria.sidebar')}>
        <div className="p-4 border-b border-gray-800">
          <div className="flex items-center gap-2">
            <Database className="w-8 h-8 text-blue-400" aria-hidden="true" />
            <div>
              <h1 className="text-xl font-bold">{t('app.name')}</h1>
              <p className="text-xs text-gray-400">{t('app.subtitle')}</p>
            </div>
          </div>
        </div>

        <nav className="p-4 flex-1" role="navigation" aria-label={t('aria.mainNav')}>
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

        {/* Language Switcher */}
        <div className="p-4 border-t border-gray-800">
          <LanguageSwitcher />
        </div>
      </aside>

      {/* Main content */}
      <main id="main-content" className="flex-1 overflow-auto" role="main" aria-label={t('aria.mainContent')} tabIndex={-1}>
        <Outlet />
      </main>
    </div>
  );
}
