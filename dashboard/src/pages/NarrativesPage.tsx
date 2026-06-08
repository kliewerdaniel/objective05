import React, { useState } from 'react';
import { useFeedStore } from '../store/feedStore';
import { useUiStore } from '../store/uiStore';
import { api } from '../api/client';
import { Stack, Warning, Flame } from '@phosphor-icons/react';

export const NarrativesPage: React.FC = () => {
  const { narratives, contradictions } = useFeedStore();
  const { openDetail, addNotification } = useUiStore();
  const [activeSubTab, setActiveSubTab] = useState<'narratives' | 'contradictions'>('narratives');
  const [resolving, setResolving] = useState<string | null>(null);

  const handleResolveContradiction = async (c: { id: string; entity_name: string }, e: React.MouseEvent) => {
    e.stopPropagation();
    const note = prompt(`Resolution note for contradiction on "${c.entity_name}":`) ?? undefined;
    setResolving(c.id);
    try {
      await api.resolveContradiction(c.id, { note: note || undefined });
      addNotification(`Contradiction resolved.`, 'success');
    } catch (err) {
      addNotification(`Failed to resolve: ${(err as Error).message}`, 'error');
    } finally {
      setResolving(null);
    }
  };

  const getStrengthColor = (strength: number) => {
    if (strength >= 0.75) return 'var(--accent-blue)';
    if (strength >= 0.5) return 'var(--accent-blue)';
    return 'var(--text-muted)';
  };

  const getSeverityColor = (severity: number) => {
    if (severity >= 0.7) return 'var(--color-danger)';
    if (severity >= 0.4) return 'var(--color-warning)';
    return 'var(--color-info)';
  };

  return (
    <div className="narratives-container animate-fade-in">
      {/* Sub tabs */}
      <div className="sub-tab-bar">
        <button
          onClick={() => setActiveSubTab('narratives')}
          className={`sub-tab-btn ${activeSubTab === 'narratives' ? 'active' : ''}`}
        >
            <Stack size={16} />
          Narrative Story Threads ({narratives.length})
        </button>
        <button
          onClick={() => setActiveSubTab('contradictions')}
          className={`sub-tab-btn ${activeSubTab === 'contradictions' ? 'active' : ''}`}
        >
          <Warning size={16} />
          Contradiction Detector ({contradictions.length})
        </button>
      </div>

      {/* Narratives Section */}
      {activeSubTab === 'narratives' && (
        <div className="narratives-section">
          {narratives.length === 0 ? (
            <div className="empty-state">
              <Stack size={48} className="text-muted" />
              <p className="heading-md">No narratives tracked yet</p>
              <p className="text-muted">The narrative engine groups related events over time into macro trends.</p>
            </div>
          ) : (
            <div className="narrative-grid">
              {narratives.map((n) => (
                <div
                  key={n.id}
                  onClick={() => openDetail('narrative', n.id)}
                  className="narrative-card"
                >
                  <div className="n-card-header">
                    <span className="badge badge-purple">{n.status}</span>
                    
                    {/* Strength Gauge */}
                    <div className="strength-widget">
                      <span className="strength-label">Strength</span>
                      <div className="strength-bar-bg">
                        <div
                          className="strength-bar-fill"
                          style={{
                            width: `${(n.strength || 0.5) * 100}%`,
                            background: getStrengthColor(n.strength || 0.5),
                          }}
                        ></div>
                      </div>
                      <span className="strength-value">{((n.strength || 0.5) * 100).toFixed(0)}%</span>
                    </div>
                  </div>

                  <h3 className="n-title">{n.title}</h3>
                  <p className="n-desc">{n.description}</p>

                  <div className="n-footer">
                    <span className="n-stat">
                      <strong>{n.event_count}</strong> correlated events
                    </span>
                    <span className="n-stat">
                      <strong>{n.sources_count || 1}</strong> unique sources
                    </span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Contradictions Section */}
      {activeSubTab === 'contradictions' && (
        <div className="contradictions-section">
          {contradictions.length === 0 ? (
            <div className="empty-state">
              <Warning size={48} className="text-muted" />
              <p className="heading-md">No contradictions detected</p>
              <p className="text-muted">Objective continuously compares assertions. Agreement is high across current inputs.</p>
            </div>
          ) : (
            <div className="contradiction-list">
              {contradictions.map((c) => (
                <div key={c.id} className="contra-card">
                  <div className="contra-header">
                    <div className="contra-entity">
                      <Flame size={16} className="text-danger" />
                      <span>Entity in conflict: <strong>{c.entity_name}</strong></span>
                    </div>
                    
                    <div className="contra-severity">
                      <span className="badge" style={{
                        backgroundColor: `rgba(239, 68, 68, 0.1)`,
                        color: getSeverityColor(c.severity || 0.5),
                        border: `1px solid rgba(239, 68, 68, 0.2)`
                      }}>
                        Severity: {((c.severity || 0.5) * 10).toFixed(1)}/10
                      </span>
                      <span className="badge badge-warning">{c.status}</span>
                    </div>
                  </div>

                  <div className="contra-claims-grid">
                    <div className="claim-box claim-a">
                      <span className="claim-label">Claim Asserted</span>
                      <p className="claim-text">"{c.claim_a}"</p>
                    </div>
                    <div className="claim-box claim-b">
                      <span className="claim-label">Competing Asserted</span>
                      <p className="claim-text">"{c.claim_b}"</p>
                    </div>
                  </div>

                  <div className="contra-footer">
                    <span className="text-muted">Detected: {new Date(c.detected_at).toLocaleString()}</span>
                    <button
                      className="btn-secondary resolve-btn"
                      onClick={(e) => handleResolveContradiction(c, e)}
                      disabled={resolving === c.id}
                    >
                      {resolving === c.id ? 'Resolving…' : 'Resolve Dispute'}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <style>{`
        .narratives-container {
          display: flex;
          flex-direction: column;
          gap: 1.5rem;
        }

        .sub-tab-bar {
          display: flex;
          gap: 1rem;
          padding: 0.75rem !important;
        }

        .sub-tab-btn {
          display: flex;
          align-items: center;
          gap: 0.5rem;
          padding: 0.5rem 1rem;
          border-radius: var(--radius-sm);
          color: var(--text-secondary);
          font-weight: 600;
          font-size: 0.9rem;
          transition: background var(--transition-fast), color var(--transition-fast);
        }

        .sub-tab-btn:hover {
          background: rgba(255, 255, 255, 0.02);
          color: var(--text-primary);
        }

        .sub-tab-btn.active {
          background: rgba(0, 122, 255, 0.08);
          color: var(--accent-blue);
        }

        /* Narratives */
        .narrative-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 1.25rem;
        }

        @media (max-width: 1024px) {
          .narrative-grid {
            grid-template-columns: 1fr;
          }
        }

        .narrative-card {
          text-align: left;
          cursor: pointer;
          display: flex;
          flex-direction: column;
          gap: 0.85rem;
        }

        .n-card-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .strength-widget {
          display: flex;
          align-items: center;
          gap: 0.5rem;
          font-size: 0.75rem;
        }

        .strength-label {
          color: var(--text-muted);
        }

        .strength-bar-bg {
          width: 60px;
          height: 5px;
          background: rgba(255, 255, 255, 0.05);
          border-radius: var(--radius-full);
          overflow: hidden;
        }

        .strength-bar-fill {
          height: 100%;
          border-radius: var(--radius-full);
        }

        .strength-value {
          color: var(--text-primary);
          font-weight: 700;
        }

        .n-title {
          font-family: var(--font-mono);
          font-size: 1.2rem;
          font-weight: 600;
          color: var(--text-primary);
        }

        .n-desc {
          font-size: 0.9rem;
          color: var(--text-secondary);
          line-height: 1.6;
          flex-grow: 1;
        }

        .n-footer {
          display: flex;
          gap: 1.5rem;
          border-top: 1px solid var(--border-color);
          padding-top: 0.75rem;
          font-size: 0.75rem;
          color: var(--text-muted);
        }

        .n-stat strong {
          color: var(--text-primary);
        }

        /* Contradictions */
        .contradiction-list {
          display: flex;
          flex-direction: column;
          gap: 1.5rem;
        }

        .contra-card {
          text-align: left;
          display: flex;
          flex-direction: column;
          gap: 1.25rem;
          border-radius: var(--radius-md) !important;
        }

        .contra-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          flex-wrap: wrap;
          gap: 0.5rem;
          border-bottom: 1px solid var(--border-color);
          padding-bottom: 0.75rem;
        }

        .contra-entity {
          display: flex;
          align-items: center;
          gap: 0.5rem;
          font-size: 0.9rem;
          color: var(--text-secondary);
        }

        .contra-entity strong {
          color: var(--text-primary);
        }

        .text-danger {
          color: var(--color-danger);
        }

        .contra-severity {
          display: flex;
          gap: 0.5rem;
        }

        .contra-claims-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 1.25rem;
        }

        @media (max-width: 768px) {
          .contra-claims-grid {
            grid-template-columns: 1fr;
          }
        }

        .claim-box {
          background: rgba(255, 255, 255, 0.02);
          border: 1px solid var(--border-color);
          border-radius: var(--radius-md);
          padding: 1rem;
          position: relative;
        }

        .claim-box::before {
          content: '';
          position: absolute;
          left: 0;
          top: 0;
          height: 100%;
          width: 3px;
          border-radius: 4px 0 0 4px;
        }

        .claim-a::before {
          background: var(--accent-blue);
        }

        .claim-b::before {
          background: var(--color-info);
        }

        .claim-label {
          font-size: 0.7rem;
          font-weight: 700;
          text-transform: uppercase;
          color: var(--text-muted);
          display: block;
          margin-bottom: 0.5rem;
        }

        .claim-text {
          font-size: 0.9rem;
          color: var(--text-primary);
          line-height: 1.5;
          font-style: italic;
        }

        .contra-footer {
          display: flex;
          justify-content: space-between;
          align-items: center;
          font-size: 0.8rem;
        }

        .resolve-btn {
          font-size: 0.8rem;
          padding: 0.35rem 0.75rem;
          border-radius: var(--radius-sm);
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
