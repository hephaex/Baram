import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatCard } from './StatCard';
import { Activity, Database, FileText } from 'lucide-react';

describe('StatCard', () => {
  it('renders title and value', () => {
    render(
      <StatCard
        title="Total Articles"
        value={1234}
        icon={FileText}
      />
    );

    expect(screen.getByText('Total Articles')).toBeInTheDocument();
    expect(screen.getByText('1,234')).toBeInTheDocument();
  });

  it('renders string value correctly', () => {
    render(
      <StatCard
        title="Status"
        value="Running"
        icon={Activity}
      />
    );

    expect(screen.getByText('Status')).toBeInTheDocument();
    expect(screen.getByText('Running')).toBeInTheDocument();
  });

  it('renders with positive change', () => {
    render(
      <StatCard
        title="Database Size"
        value="500MB"
        icon={Database}
        change="+10% from last week"
        changeType="positive"
      />
    );

    const changeElement = screen.getByText('+10% from last week');
    expect(changeElement).toBeInTheDocument();
    expect(changeElement).toHaveClass('text-green-600');
  });

  it('renders with negative change', () => {
    render(
      <StatCard
        title="Error Rate"
        value="5%"
        icon={Activity}
        change="-2% from yesterday"
        changeType="negative"
      />
    );

    const changeElement = screen.getByText('-2% from yesterday');
    expect(changeElement).toBeInTheDocument();
    expect(changeElement).toHaveClass('text-red-600');
  });

  it('renders with neutral change by default', () => {
    render(
      <StatCard
        title="Requests"
        value={100}
        icon={Activity}
        change="Same as yesterday"
      />
    );

    const changeElement = screen.getByText('Same as yesterday');
    expect(changeElement).toBeInTheDocument();
    expect(changeElement).toHaveClass('text-gray-500');
  });

  it('does not render change when not provided', () => {
    render(
      <StatCard
        title="Count"
        value={50}
        icon={FileText}
      />
    );

    // Check that no change text is rendered
    const container = screen.getByText('Count').closest('div');
    expect(container?.textContent).not.toContain('%');
  });

  it('formats large numbers with locale string', () => {
    render(
      <StatCard
        title="Total"
        value={1000000}
        icon={Database}
      />
    );

    // Should be formatted as "1,000,000"
    expect(screen.getByText('1,000,000')).toBeInTheDocument();
  });

  it('renders the icon', () => {
    const { container } = render(
      <StatCard
        title="Test"
        value={0}
        icon={Activity}
      />
    );

    // Check that SVG icon is rendered
    const svg = container.querySelector('svg');
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveClass('w-6', 'h-6', 'text-blue-600');
  });
});
