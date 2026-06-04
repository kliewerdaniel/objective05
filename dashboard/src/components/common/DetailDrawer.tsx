import React from 'react';
import { useUiStore } from '../../store/uiStore';
import { useFeedStore } from '../../store/feedStore';
import { X, User, MessageSquare, Tag, Globe } from 'lucide-react';

export const DetailDrawer: React.FC = () => {
  const { activeDetailId, activeDetailType, closeDetail, openDetail } = useUiStore();
  const { documents, extractions, events } = useFeedStore();

  if (!activeDetailId || !activeDetailType) return null;

  // Retrieve details based on type
  const getDocumentDetails = () => {
    const doc = documents.find((d) => String(d.id) === activeDetailId);
    const ext = extractions.find((e) => e.document_id === activeDetailId);
    return { doc, ext };
  };

  const getEventDetails = () => {
    return events.find((e) => e.id.toString() === activeDetailId);
  };

  const getEntityDetails = () => {
    // Find entity by name (the id is the name in this case)
    const entityName = activeDetailId;
    let type = 'Concept';
    let mentionsCount = 0;
    const relatedClaims: string[] = [];

    extractions.forEach((ext) => {
      const found = ext.entities.find((e) => e.name === entityName);
      if (found) {
        type = found.entity_type;
        mentionsCount++;
      }
      ext.claims.forEach((c) => {
        if (c.subject_name === entityName || c.object_name === entityName) {
          relatedClaims.push(c.claim_text);
        }
      });
    });

    return { name: entityName, type, mentionsCount, claims: relatedClaims };
  };

  const renderContent = () => {
    switch (activeDetailType) {
      case 'document': {
        const { doc, ext } = getDocumentDetails();
        if (!doc) return <p>Document not found.</p>;
        return (
          <div className="drawer-inner-content">
            <h2 className="heading-lg mb-2">{doc.title || 'Untitled Document'}</h2>
            <div className="drawer-meta mb-4">
              <span className="badge badge-purple">{doc.source_type}</span>
              <span className="badge badge-info">{doc.source_id}</span>
              {doc.author && (
                <span className="meta-item">
                  <User size={13} /> {doc.author}
                </span>
              )}
            </div>

            <div className="section-block">
              <h4 className="section-title">Raw Content</h4>
              <div className="document-body-text">{doc.body}</div>
            </div>

            {ext && ext.entities.length > 0 && (
              <div className="section-block">
                <h4 className="section-title">Extracted Entities</h4>
                <div className="entities-row">
                  {ext.entities.map((ent, idx) => (
                    <button
                      key={idx}
                      onClick={() => openDetail('entity', ent.name)}
                      className="entity-tag-btn"
                    >
                      <Tag size={12} />
                      {ent.name} ({ent.entity_type})
                    </button>
                  ))}
                </div>
              </div>
            )}

            {ext && ext.claims.length > 0 && (
              <div className="section-block">
                <h4 className="section-title">Asserted Claims ({ext.claims.length})</h4>
                <div className="claims-list">
                  {ext.claims.map((claim, idx) => (
                    <div key={idx} className="claim-item">
                      <p className="claim-text">"{claim.claim_text}"</p>
                      <div className="claim-meta">
                        <span>Subject: <strong>{claim.subject_name}</strong></span>
                        <span>Confidence: <strong>{claim.confidence.toFixed(2)}</strong></span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        );
      }
      
      case 'event': {
        const ev = getEventDetails();
        if (!ev) return <p>Event not found.</p>;
        return (
          <div className="drawer-inner-content">
            <h2 className="heading-lg mb-2">{ev.title}</h2>
            <div className="drawer-meta mb-4">
              <span className="badge badge-purple">{ev.event_type}</span>
              <span className="badge badge-success">{ev.status}</span>
              {ev.location && (
                <span className="meta-item">
                  <Globe size={13} /> {ev.location}
                </span>
              )}
            </div>

            <div className="section-block">
              <h4 className="section-title">Synthesized Intelligence Summary</h4>
              <p className="event-full-desc">{ev.description}</p>
            </div>

            <div className="section-block">
              <h4 className="section-title">Participating Entities</h4>
              <div className="entities-row">
                {ev.participating_entities.map((name, idx) => (
                  <button
                    key={idx}
                    onClick={() => openDetail('entity', name)}
                    className="entity-tag-btn"
                  >
                    {name}
                  </button>
                ))}
              </div>
            </div>

            <div className="section-block">
              <h4 className="section-title">Event Telemetry</h4>
              <div className="telemetry-grid">
                <div className="tele-metric">
                  <span className="metric-label">Importance</span>
                  <span className="metric-val">{(ev.importance * 10).toFixed(1)}/10</span>
                </div>
                <div className="tele-metric">
                  <span className="metric-label">Confidence</span>
                  <span className="metric-val">{(ev.confidence * 100).toFixed(0)}%</span>
                </div>
                <div className="tele-metric">
                  <span className="metric-label">Sources Diversity</span>
                  <span className="metric-val">{ev.source_diversity.toFixed(1)}</span>
                </div>
                <div className="tele-metric">
                  <span className="metric-label">Claims Folded</span>
                  <span className="metric-val">{ev.claim_count}</span>
                </div>
              </div>
            </div>
          </div>
        );
      }

      case 'entity': {
        const ent = getEntityDetails();
        return (
          <div className="drawer-inner-content">
            <h2 className="heading-lg mb-2">{ent.name}</h2>
            <div className="drawer-meta mb-4">
              <span className="badge badge-purple">{ent.type}</span>
              <span className="meta-item">
                <MessageSquare size={13} /> Mentions: {ent.mentionsCount}
              </span>
            </div>

            <div className="section-block">
              <h4 className="section-title">Associated Claims Across All Ingests</h4>
              {ent.claims.length === 0 ? (
                <p className="text-muted">No associated claims found in database.</p>
              ) : (
                <div className="claims-list">
                  {ent.claims.map((claimText, idx) => (
                    <div key={idx} className="claim-item">
                      <p className="claim-text">"{claimText}"</p>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        );
      }

      default:
        return <p>Details not available.</p>;
    }
  };

  return (
    <div className="drawer-overlay" onClick={closeDetail}>
      <div className="drawer-sheet glass-panel" onClick={(e) => e.stopPropagation()}>
        <div className="drawer-header">
          <span className="drawer-type-label">{activeDetailType} details</span>
          <button onClick={closeDetail} className="close-btn">
            <X size={20} />
          </button>
        </div>

        <div className="drawer-body">{renderContent()}</div>
      </div>

      <style>{`
        .drawer-overlay {
          position: fixed;
          inset: 0;
          background: rgba(0, 0, 0, 0.4);
          backdrop-filter: blur(4px);
          z-index: 100;
          display: flex;
          justify-content: flex-end;
          animation: fadeInOverlay 0.2s ease-out;
        }

        .drawer-sheet {
          width: 500px;
          height: 100%;
          border-radius: 0 !important;
          border-top: none;
          border-bottom: none;
          border-right: none;
          display: flex;
          flex-direction: column;
          box-shadow: -10px 0 30px rgba(0, 0, 0, 0.2);
          animation: slideInSheet 0.25s cubic-bezier(0.16, 1, 0.3, 1);
        }

        @media (max-width: 640px) {
          .drawer-sheet {
            width: 100%;
          }
        }

        .drawer-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding-bottom: 1rem;
          border-bottom: 1px solid var(--border-color);
        }

        .drawer-type-label {
          font-size: 0.75rem;
          font-weight: 700;
          text-transform: uppercase;
          letter-spacing: 0.08em;
          color: var(--accent-primary);
        }

        .drawer-body {
          flex-grow: 1;
          overflow-y: auto;
          padding: 1.5rem 0;
        }

        .drawer-inner-content {
          text-align: left;
        }

        .drawer-meta {
          display: flex;
          gap: 0.75rem;
          align-items: center;
          flex-wrap: wrap;
        }

        .section-block {
          margin-bottom: 1.75rem;
        }

        .section-title {
          font-size: 0.75rem;
          font-weight: 700;
          text-transform: uppercase;
          letter-spacing: 0.05em;
          color: var(--text-muted);
          margin-bottom: 0.75rem;
          border-bottom: 1px solid rgba(255, 255, 255, 0.03);
          padding-bottom: 0.25rem;
        }

        .document-body-text {
          font-size: 0.9rem;
          color: var(--text-secondary);
          line-height: 1.6;
          background: rgba(255, 255, 255, 0.01);
          border: 1px solid var(--border-color);
          padding: 1rem;
          border-radius: var(--radius-md);
          max-height: 250px;
          overflow-y: auto;
        }

        .entities-row {
          display: flex;
          flex-wrap: wrap;
          gap: 0.5rem;
        }

        .entity-tag-btn {
          display: inline-flex;
          align-items: center;
          gap: 0.35rem;
          font-size: 0.8rem;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          padding: 0.35rem 0.75rem;
          border-radius: var(--radius-sm);
          color: var(--text-primary);
          transition: border-color var(--transition-fast);
        }

        .entity-tag-btn:hover {
          border-color: var(--accent-primary);
        }

        .claims-list {
          display: flex;
          flex-direction: column;
          gap: 0.75rem;
        }

        .claim-item {
          background: rgba(255, 255, 255, 0.01);
          border: 1px solid var(--border-color);
          padding: 0.85rem;
          border-radius: var(--radius-md);
        }

        .claim-text {
          font-size: 0.85rem;
          color: var(--text-primary);
          font-style: italic;
          line-height: 1.5;
        }

        .claim-meta {
          display: flex;
          justify-content: space-between;
          font-size: 0.75rem;
          color: var(--text-muted);
          margin-top: 0.5rem;
          border-top: 1px solid rgba(255, 255, 255, 0.03);
          padding-top: 0.35rem;
        }

        .event-full-desc {
          font-size: 0.95rem;
          color: var(--text-secondary);
          line-height: 1.6;
        }

        .telemetry-grid {
          display: grid;
          grid-template-columns: repeat(4, 1fr);
          gap: 0.75rem;
        }

        @media (max-width: 480px) {
          .telemetry-grid {
            grid-template-columns: 1fr 1fr;
          }
        }

        .tele-metric {
          background: rgba(255, 255, 255, 0.01);
          border: 1px solid var(--border-color);
          border-radius: var(--radius-sm);
          padding: 0.5rem;
          text-align: center;
        }

        .metric-label {
          display: block;
          font-size: 0.65rem;
          font-weight: 700;
          color: var(--text-muted);
          text-transform: uppercase;
          margin-bottom: 0.25rem;
        }

        .metric-val {
          font-size: 1rem;
          font-weight: 700;
          color: var(--text-primary);
          font-family: var(--font-display);
        }

        @keyframes fadeInOverlay {
          from { opacity: 0; }
          to { opacity: 1; }
        }

        @keyframes slideInSheet {
          from { transform: translateX(100%); }
          to { transform: translateX(0); }
        }
      `}</style>
    </div>
  );
};
