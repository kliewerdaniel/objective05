import React from 'react';
import { useFeedStore } from '../../store/feedStore';
import { Database, Cpu, Question, HardDrive } from '@phosphor-icons/react';

export const Footer: React.FC = () => {
  const { metrics, documents, extractions, events } = useFeedStore();

  const getMetricValue = (key: 'ingested' | 'extracted' | 'events' | 'cycles') => {
    if (metrics) {
      switch (key) {
        case 'ingested': return metrics.documents_ingested;
        case 'extracted': return metrics.extractions_completed;
        case 'events': return metrics.events_created;
        case 'cycles': return metrics.pipeline_cycles;
      }
    }
    // Fallback to local counts
    switch (key) {
      case 'ingested': return documents.length;
      case 'extracted': return extractions.length;
      case 'events': return events.length;
      case 'cycles': return 0;
    }
  };

  return (
    <footer className="footer">
      <div className="footer-left">
        <div className="stat-ticker">
          <Database size={14} className="text-muted" />
          <span>Ingested: <strong>{getMetricValue('ingested')}</strong> docs</span>
        </div>
        <div className="stat-ticker">
          <Cpu size={14} className="text-muted" />
          <span>Extracted: <strong>{getMetricValue('extracted')}</strong> claims</span>
        </div>
        <div className="stat-ticker">
          <HardDrive size={14} className="text-muted" />
          <span>Correlated: <strong>{getMetricValue('events')}</strong> events</span>
        </div>
      </div>
      
      <div className="footer-right">
        {metrics?.pipeline_cycles !== undefined && (
          <span className="pipeline-cycle">
            Pipeline cycles: <strong>{getMetricValue('cycles')}</strong>
          </span>
        )}
        <span className="footer-link">
          <Question size={14} />
          Local OS
        </span>
      </div>

      <style>{`
        .footer {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 0.5rem 1.5rem;
          background: var(--bg-secondary);
          border-top: 1px solid var(--border-color);
          font-size: 0.7rem;
          color: var(--text-muted);
          z-index: 30;
        }

        .footer-left {
          display: flex;
          align-items: center;
          gap: 1.25rem;
        }

        .stat-ticker {
          display: flex;
          align-items: center;
          gap: 0.35rem;
        }

        .stat-ticker strong {
          color: var(--text-secondary);
          font-weight: 500;
        }

        .footer-right {
          display: flex;
          align-items: center;
          gap: 1rem;
        }

        .pipeline-cycle strong {
          color: var(--text-secondary);
          font-weight: 500;
        }

        .footer-link {
          display: flex;
          align-items: center;
          gap: 0.3rem;
          cursor: help;
        }
      `}</style>
    </footer>
  );
};
