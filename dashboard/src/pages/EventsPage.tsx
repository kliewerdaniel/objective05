import React, { useState } from 'react';
import { useFeedStore } from '../store/feedStore';
import { useUiStore } from '../store/uiStore';
import { api } from '../api/client';
import { TrendingUp, Calendar, MessageSquare, ArrowUpDown, CheckCircle2 } from 'lucide-react';
import type { DerivedEvent } from '../api/types';

export const EventsPage: React.FC = () => {
  const { events } = useFeedStore();
  const { openDetail, addNotification } = useUiStore();
  const [filterType, setFilterType] = useState('all');
  const [sortBy, setSortBy] = useState<'importance' | 'date'>('importance');
  const [resolving, setResolving] = useState<string | null>(null);

  const handleResolve = async (ev: DerivedEvent, e: React.MouseEvent) => {
    e.stopPropagation();
    if (!confirm(`Mark event "${ev.title}" as resolved?`)) return;
    setResolving(ev.id);
    try {
      await api.resolveEvent(ev.id, { note: 'Resolved from dashboard' });
      addNotification(`Event "${ev.title}" resolved.`, 'success');
    } catch (err: any) {
      addNotification(`Failed to resolve: ${err.message}`, 'error');
    } finally {
      setResolving(null);
    }
  };

  // Filter & Sort
  const filteredEvents = events
    .filter((ev: DerivedEvent) => filterType === 'all' || ev.event_type.toLowerCase() === filterType)
    .sort((a: DerivedEvent, b: DerivedEvent) => {
      if (sortBy === 'importance') {
        return b.importance - a.importance;
      } else {
        return new Date(b.first_observed_at).getTime() - new Date(a.first_observed_at).getTime();
      }
    });

  const getImportanceColor = (score: number) => {
    if (score >= 0.75) return 'var(--color-danger)';
    if (score >= 0.5) return 'var(--color-warning)';
    return 'var(--accent-secondary)';
  };

  const getEventTypeLabel = (type: string) => {
    return type.charAt(0).toUpperCase() + type.slice(1);
  };

  return (
    <div className="events-container animate-fade-in">
      {/* Filtering Header */}
      <div className="filter-header glass-card">
        <div className="filter-pills">
          {['all', 'technology', 'business', 'politics', 'science', 'world'].map((type: string) => (
            <button
              key={type}
              onClick={() => setFilterType(type)}
              className={`pill-btn ${filterType === type ? 'active' : ''}`}
            >
              {type === 'all' ? 'All Beats' : getEventTypeLabel(type)}
            </button>
          ))}
        </div>

        <div className="sort-controls">
          <button
            onClick={() => setSortBy(sortBy === 'importance' ? 'date' : 'importance')}
            className="btn-secondary sort-btn"
          >
            <ArrowUpDown size={14} />
            Sort by: {sortBy === 'importance' ? 'Importance' : 'Recency'}
          </button>
        </div>
      </div>

      {/* Events Grid */}
      {filteredEvents.length === 0 ? (
        <div className="empty-state glass-card">
          <TrendingUp size={48} className="text-muted" />
          <p className="heading-md">No events formed yet</p>
          <p className="text-muted">Ingest more documents and run the correlation engine to group claims into events.</p>
        </div>
      ) : (
        <div className="events-grid">
          {filteredEvents.map((ev: DerivedEvent) => (
            <div
              key={ev.id}
              onClick={() => openDetail('event', ev.id)}
              className="event-card glass-card"
            >
              <div className="event-card-header">
                <span className="badge badge-purple">{ev.event_type}</span>
                <span className="badge badge-success">{ev.status}</span>
                
                {/* Importance Bar */}
                <div className="importance-meter" title={`Importance: ${(ev.importance * 100).toFixed(0)}%`}>
                  <div
                    className="importance-fill"
                    style={{
                      width: `${ev.importance * 100}%`,
                      backgroundColor: getImportanceColor(ev.importance),
                    }}
                  ></div>
                </div>
              </div>

              <h3 className="event-title">{ev.title}</h3>
              <p className="event-desc">{ev.description}</p>

              <div className="event-card-footer">
                <div className="event-meta">
                  <span className="meta-item">
                    <Calendar size={13} />
                    {new Date(ev.first_observed_at).toLocaleDateString(undefined, {
                      month: 'short',
                      day: 'numeric',
                    })}
                  </span>
                  <span className="meta-item">
                    <MessageSquare size={13} />
                    {ev.claim_count} claims
                  </span>
                </div>

                <div className="participating-entities">
                  {ev.participating_entities.slice(0, 3).map((entity: string, i: number) => (
                    <span key={i} className="entity-chip">
                      {entity}
                    </span>
                  ))}
                  {ev.participating_entities.length > 3 && (
                    <span className="entity-chip-more">
                      +{ev.participating_entities.length - 3}
                    </span>
                  )}
                  {ev.status !== 'resolved' && (
                    <button
                      className="btn-secondary event-resolve-btn"
                      onClick={(e) => handleResolve(ev, e)}
                      disabled={resolving === ev.id}
                      title="Mark as resolved"
                    >
                      <CheckCircle2 size={12} />
                      {resolving === ev.id ? 'Resolving…' : 'Resolve'}
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      <style>{`
        .events-container {
          display: flex;
          flex-direction: column;
          gap: 1.5rem;
        }

        .filter-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 1rem !important;
          flex-wrap: wrap;
          gap: 1rem;
        }

        .filter-pills {
          display: flex;
          gap: 0.5rem;
          flex-wrap: wrap;
        }

        .pill-btn {
          padding: 0.4rem 1rem;
          border-radius: var(--radius-full);
          font-size: 0.85rem;
          font-weight: 500;
          color: var(--text-secondary);
          background: rgba(255, 255, 255, 0.02);
          border: 1px solid var(--border-color);
          transition: background var(--transition-fast), color var(--transition-fast);
        }

        .pill-btn:hover {
          background: rgba(255, 255, 255, 0.05);
          color: var(--text-primary);
        }

        .pill-btn.active {
          background: var(--accent-gradient);
          color: var(--text-primary);
          border-color: transparent;
        }

        .sort-btn {
          font-size: 0.85rem;
          padding: 0.4rem 1rem;
        }

        .events-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
          gap: 1.25rem;
        }

        @media (max-width: 640px) {
          .events-grid {
            grid-template-columns: 1fr;
          }
        }

        .event-card {
          display: flex;
          flex-direction: column;
          gap: 0.85rem;
          text-align: left;
          cursor: pointer;
        }

        .event-card-header {
          display: flex;
          align-items: center;
          gap: 0.5rem;
          position: relative;
        }

        .importance-meter {
          flex-grow: 1;
          height: 4px;
          background: rgba(255, 255, 255, 0.05);
          border-radius: var(--radius-full);
          margin-left: 0.5rem;
          overflow: hidden;
        }

        .importance-fill {
          height: 100%;
          border-radius: var(--radius-full);
        }

        .event-title {
          font-family: var(--font-display);
          font-size: 1.15rem;
          font-weight: 600;
          color: var(--text-primary);
        }

        .event-desc {
          font-size: 0.9rem;
          color: var(--text-secondary);
          line-height: 1.5;
          flex-grow: 1;
        }

        .event-card-footer {
          display: flex;
          justify-content: space-between;
          align-items: center;
          border-top: 1px solid rgba(255, 255, 255, 0.04);
          padding-top: 0.75rem;
          flex-wrap: wrap;
          gap: 0.5rem;
        }

        .event-meta {
          display: flex;
          gap: 0.75rem;
          font-size: 0.75rem;
          color: var(--text-muted);
        }

        .participating-entities {
          display: flex;
          gap: 0.35rem;
          align-items: center;
        }

        .entity-chip {
          font-size: 0.7rem;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          padding: 0.15rem 0.45rem;
          border-radius: 4px;
          color: var(--text-secondary);
          white-space: nowrap;
          max-width: 90px;
          overflow: hidden;
          text-overflow: ellipsis;
        }

        .entity-chip-more {
          font-size: 0.7rem;
          color: var(--accent-secondary);
          font-weight: 600;
        }

        .event-resolve-btn {
          font-size: 0.7rem;
          padding: 0.2rem 0.55rem;
          display: inline-flex;
          gap: 0.25rem;
          align-items: center;
          margin-left: 0.5rem;
        }

        .empty-state {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          padding: 4rem 2rem !important;
          gap: 1rem;
          text-align: center;
        }
      `}</style>
    </div>
  );
};
