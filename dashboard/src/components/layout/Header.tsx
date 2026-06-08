import React from 'react';
import { useUiStore } from '../../store/uiStore';
import { useFeedStore } from '../../store/feedStore';
import { ArrowClockwise, WifiHigh, WifiSlash, Clock } from '@phosphor-icons/react';

export const Header: React.FC = () => {
  const { activePage } = useUiStore();
  const { loading, fetchData, wsConnected, metrics, health } = useFeedStore();

  const getPageTitle = () => {
    switch (activePage) {
      case 'feed':
        return 'Intelligence Feed';
      case 'events':
        return 'Event Correlation';
      case 'narratives':
        return 'Narrative Clusters';
      case 'graph':
        return 'Knowledge Graph';
      case 'broadcasts':
        return 'Continuous Broadcasts';
      case 'sources':
        return 'Ingestion Sources';
      case 'settings':
        return 'System Configuration';
      default:
        return 'Objective';
    }
  };

  const getUptimeString = () => {
    if (!health?.uptime_secs && !metrics?.uptime_secs) return '0m';
    const totalSecs = health?.uptime_secs || metrics?.uptime_secs || 0;
    const hrs = Math.floor(totalSecs / 3600);
    const mins = Math.floor((totalSecs % 3600) / 60);
    if (hrs > 0) return `${hrs}h ${mins}m`;
    return `${mins}m`;
  };

  return (
    <header className="header">
      <div className="header-left">
        <h1 className="header-title">{getPageTitle()}</h1>
      </div>

      <div className="header-right">
        {/* Connection status */}
        <div className="connection-status" title={wsConnected ? 'WebSocket Active' : 'Polling REST API'}>
          {wsConnected ? (
            <>
              <WifiHigh size={16} className="text-success animate-pulse-slow" />
              <span className="connection-label">Live</span>
            </>
          ) : (
            <>
              <WifiSlash size={16} className="text-warning" />
              <span className="connection-label text-warning">Syncing</span>
            </>
          )}
        </div>

        {/* Uptime widget */}
        <div className="uptime-widget">
          <Clock size={15} className="text-muted" />
          <span>Uptime: <strong>{getUptimeString()}</strong></span>
        </div>

        {/* Sync Button */}
        <button
          onClick={() => fetchData()}
          disabled={loading}
          className={`sync-btn ${loading ? 'loading' : ''}`}
          title="Manual Sync"
        >
          <ArrowClockwise size={16} />
        </button>
      </div>

      <style>{`
        .header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 0.75rem 1.5rem;
          border-bottom: 1px solid var(--border-color);
          background: var(--bg-secondary);
          z-index: 40;
        }

        .header-title {
          font-family: var(--font-mono);
          font-weight: 600;
          font-size: 1rem;
          color: var(--text-primary);
          letter-spacing: 0.02em;
        }

        .header-right {
          display: flex;
          align-items: center;
          gap: 0.75rem;
        }

        .connection-status {
          display: flex;
          align-items: center;
          gap: 0.35rem;
          font-size: 0.75rem;
          font-weight: 600;
          background: var(--bg-tertiary);
          padding: 0.25rem 0.6rem;
          border-radius: var(--radius-sm);
          border: 1px solid var(--border-color);
        }

        .text-success {
          color: var(--color-success);
        }

        .text-warning {
          color: var(--color-warning);
        }

        .connection-label {
          color: var(--text-secondary);
        }

        .uptime-widget {
          display: flex;
          align-items: center;
          gap: 0.4rem;
          font-size: 0.75rem;
          color: var(--text-secondary);
          background: var(--bg-tertiary);
          padding: 0.25rem 0.6rem;
          border-radius: var(--radius-sm);
          border: 1px solid var(--border-color);
        }

        .sync-btn {
          color: var(--text-secondary);
          display: flex;
          align-items: center;
          justify-content: center;
          padding: 0.4rem;
          border-radius: var(--radius-sm);
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          transition: background var(--transition-fast), color var(--transition-fast);
        }

        .sync-btn:hover:not(:disabled) {
          background: var(--bg-elevated);
          color: var(--text-primary);
        }

        .sync-btn.loading svg {
          animation: spin 1s linear infinite;
        }

        @keyframes spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }

        @keyframes pulse-slow {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.6; }
        }
        .animate-pulse-slow {
          animation: pulse-slow 2s infinite ease-in-out;
        }
      `}</style>
    </header>
  );
};
