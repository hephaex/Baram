/**
 * StatCard component with React.memo optimization and accessibility
 * Issue #21: WCAG Accessibility Compliance
 * Issue #35: React.memo/useMemo performance optimization
 */
import { memo, useId } from 'react';
import type { LucideIcon } from 'lucide-react';

interface StatCardProps {
  title: string;
  value: string | number;
  icon: LucideIcon;
  change?: string;
  changeType?: 'positive' | 'negative' | 'neutral';
}

export const StatCard = memo(function StatCard({
  title,
  value,
  icon: Icon,
  change,
  changeType = 'neutral',
}: StatCardProps) {
  const titleId = useId();
  const changeColors = {
    positive: 'text-green-600',
    negative: 'text-red-600',
    neutral: 'text-gray-500',
  };

  const changeAriaLabel = changeType === 'positive' ? '증가' : changeType === 'negative' ? '감소' : '';

  return (
    <article
      className="bg-white rounded-xl shadow-sm p-6"
      role="region"
      aria-labelledby={titleId}
    >
      <div className="flex items-center justify-between">
        <div>
          <h3 id={titleId} className="text-sm text-gray-500 font-medium">{title}</h3>
          <p className="text-2xl font-bold mt-1" aria-label={`${title}: ${value.toLocaleString()}`}>
            {value.toLocaleString()}
          </p>
          {change && (
            <p
              className={`text-sm mt-1 ${changeColors[changeType]}`}
              aria-label={changeAriaLabel ? `${changeAriaLabel}: ${change}` : undefined}
            >
              {change}
            </p>
          )}
        </div>
        <div className="p-3 bg-blue-50 rounded-lg" aria-hidden="true">
          <Icon className="w-6 h-6 text-blue-600" />
        </div>
      </div>
    </article>
  );
});
