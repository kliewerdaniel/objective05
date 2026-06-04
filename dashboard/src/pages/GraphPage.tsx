import React, { useEffect, useRef, useState } from 'react';
import { useFeedStore } from '../store/feedStore';
import { useUiStore } from '../store/uiStore';
import { ZoomIn, ZoomOut, RotateCcw, Search } from 'lucide-react';

interface Node {
  id: string;
  label: string;
  type: 'entity' | 'event' | 'document';
  subType?: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
  fx?: number | null;
  fy?: number | null;
  radius: number;
}

interface Link {
  source: string;
  target: string;
  type: string;
}

export const GraphPage: React.FC = () => {
  const { events, extractions, documents } = useFeedStore();
  const { openDetail } = useUiStore();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  
  const [nodes, setNodes] = useState<Node[]>([]);
  const [links, setLinks] = useState<Link[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedNode, setSelectedNode] = useState<Node | null>(null);
  
  // Viewport transformation
  const [transform, setTransform] = useState({ x: 0, y: 0, zoom: 1 });
  const [isDragging, setIsDragging] = useState(false);
  const [draggedNodeIndex, setDraggedNodeIndex] = useState<number | null>(null);
  const dragStart = useRef({ x: 0, y: 0 });

  // Generate nodes and links from store data on mount
  useEffect(() => {
    const nodeMap = new Map<string, Node>();
    const linkList: Link[] = [];

    // Add seeded events as Nodes
    events.forEach((ev) => {
      const id = `event-${ev.id}`;
      nodeMap.set(id, {
        id,
        label: ev.title,
        type: 'event',
        x: Math.random() * 600 + 100,
        y: Math.random() * 400 + 100,
        vx: 0,
        vy: 0,
        radius: 14,
      });

      // Add entities participating in the event
      ev.participating_entities.forEach((entName) => {
        const entId = `entity-${entName}`;
        if (!nodeMap.has(entId)) {
          nodeMap.set(entId, {
            id: entId,
            label: entName,
            type: 'entity',
            subType: entName === 'Austin' ? 'location' : entName.includes('Inc') ? 'organization' : 'concept',
            x: Math.random() * 600 + 100,
            y: Math.random() * 400 + 100,
            vx: 0,
            vy: 0,
            radius: 10,
          });
        }
        linkList.push({
          source: id,
          target: entId,
          type: 'PARTICIPATES_IN',
        });
      });
    });

    // Add raw documents as Nodes if extractions exist
    extractions.forEach((ext) => {
      const doc = documents.find((d) => String(d.id) === ext.document_id);
      if (!doc) return;
      const docId = `doc-${doc.id}`;
      
      nodeMap.set(docId, {
        id: docId,
        label: doc.title || 'Document',
        type: 'document',
        x: Math.random() * 600 + 100,
        y: Math.random() * 400 + 100,
        vx: 0,
        vy: 0,
        radius: 12,
      });

      // Link doc to entities extracted from it
      ext.entities.forEach((ent) => {
        const entId = `entity-${ent.name}`;
        if (!nodeMap.has(entId)) {
          nodeMap.set(entId, {
            id: entId,
            label: ent.name,
            type: 'entity',
            subType: ent.entity_type,
            x: Math.random() * 600 + 100,
            y: Math.random() * 400 + 100,
            vx: 0,
            vy: 0,
            radius: 10,
          });
        }
        linkList.push({
          source: docId,
          target: entId,
          type: 'MENTIONS',
        });
      });
    });

    setNodes(Array.from(nodeMap.values()));
    setLinks(linkList);
    
    // Auto-center viewport
    if (canvasRef.current) {
      const rect = canvasRef.current.getBoundingClientRect();
      setTransform({ x: rect.width / 2 - 400, y: rect.height / 2 - 300, zoom: 0.95 });
    }
  }, [events, extractions, documents]);

  // Force-directed layout physics loop
  useEffect(() => {
    if (nodes.length === 0) return;

    let animationFrameId: number;
    const repulsionStrength = 220;
    const attractionStrength = 0.045;
    const damping = 0.85;
    const gravity = 0.02;

    const tick = () => {
      // 1. Repulsion between all nodes
      for (let i = 0; i < nodes.length; i++) {
        const nodeA = nodes[i];
        for (let j = i + 1; j < nodes.length; j++) {
          const nodeB = nodes[j];
          const dx = nodeB.x - nodeA.x;
          const dy = nodeB.y - nodeA.y;
          const distSq = dx * dx + dy * dy + 0.1;
          const dist = Math.sqrt(distSq);

          if (dist < 400) {
            const force = repulsionStrength / distSq;
            const fx = (dx / dist) * force;
            const fy = (dy / dist) * force;

            nodeA.vx -= fx;
            nodeA.vy -= fy;
            nodeB.vx += fx;
            nodeB.vy += fy;
          }
        }
      }

      // 2. Attraction between connected nodes
      links.forEach((link) => {
        const nodeA = nodes.find((n) => n.id === link.source);
        const nodeB = nodes.find((n) => n.id === link.target);
        if (nodeA && nodeB) {
          const dx = nodeB.x - nodeA.x;
          const dy = nodeB.y - nodeA.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 1;
          const restLength = 100;
          const force = (dist - restLength) * attractionStrength;
          const fx = (dx / dist) * force;
          const fy = (dy / dist) * force;

          nodeA.vx += fx;
          nodeA.vy += fy;
          nodeB.vx -= fx;
          nodeB.vy -= fy;
        }
      });

      // 3. Gravity pulling toward center (e.g. 400, 300)
      const centerX = 400;
      const centerY = 300;
      nodes.forEach((node) => {
        const dx = centerX - node.x;
        const dy = centerY - node.y;
        node.vx += dx * gravity;
        node.vy += dy * gravity;

        // Apply forces to positions
        if (node.fx !== undefined && node.fx !== null) {
          node.x = node.fx;
          node.vx = 0;
        } else {
          node.x += node.vx;
          node.vx *= damping;
        }

        if (node.fy !== undefined && node.fy !== null) {
          node.y = node.fy;
          node.vy = 0;
        } else {
          node.y += node.vy;
          node.vy *= damping;
        }
      });

      // Render the graph
      render();
      animationFrameId = requestAnimationFrame(tick);
    };

    const render = () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext('2d');
      if (!ctx) return;

      // Clear with background color
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.save();

      // Viewport transform
      ctx.translate(transform.x, transform.y);
      ctx.scale(transform.zoom, transform.zoom);

      // Draw Links/Edges
      links.forEach((link) => {
        const nodeA = nodes.find((n) => n.id === link.source);
        const nodeB = nodes.find((n) => n.id === link.target);
        if (nodeA && nodeB) {
          ctx.beginPath();
          ctx.moveTo(nodeA.x, nodeA.y);
          ctx.lineTo(nodeB.x, nodeB.y);
          ctx.strokeStyle = 'rgba(255, 255, 255, 0.08)';
          ctx.lineWidth = 1.5;
          ctx.stroke();
        }
      });

      // Draw Nodes
      nodes.forEach((node) => {
        const isMatched = searchQuery
          ? node.label.toLowerCase().includes(searchQuery.toLowerCase())
          : true;
        const isSelected = selectedNode?.id === node.id;

        ctx.beginPath();
        ctx.arc(node.x, node.y, node.radius, 0, 2 * Math.PI);

        // Styling based on node type
        let color = '#94a3b8';
        if (node.type === 'event') {
          color = '#3b82f6'; // Event: Blue
        } else if (node.type === 'document') {
          color = '#8b5cf6'; // Doc: Purple
        } else if (node.type === 'entity') {
          if (node.subType === 'location' || node.subType === 'location') color = '#10b981'; // Green
          else if (node.subType === 'organization') color = '#06b6d4'; // Cyan
          else color = '#ec4899'; // Concept: Pink
        }

        ctx.fillStyle = color;
        ctx.shadowBlur = isSelected ? 15 : 0;
        ctx.shadowColor = color;
        ctx.globalAlpha = isMatched ? 1.0 : 0.25;
        ctx.fill();
        ctx.globalAlpha = 1.0;
        ctx.shadowBlur = 0; // Reset

        // Node label
        ctx.fillStyle = isSelected ? '#ffffff' : 'rgba(255, 255, 255, 0.75)';
        ctx.font = isSelected ? 'bold 11px sans-serif' : '10px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(node.label, node.x, node.y - node.radius - 6);
      });

      ctx.restore();
    };

    tick();

    return () => {
      cancelAnimationFrame(animationFrameId);
    };
  }, [nodes, links, transform, searchQuery, selectedNode]);

  // Handle Mouse Events for Pan/Zoom & Drag Node
  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const clientX = e.clientX - rect.left;
    const clientY = e.clientY - rect.top;

    // Convert mouse to graph space
    const graphX = (clientX - transform.x) / transform.zoom;
    const graphY = (clientY - transform.y) / transform.zoom;

    // Check if clicked a node
    const clickedNodeIndex = nodes.findIndex((node) => {
      const dx = node.x - graphX;
      const dy = node.y - graphY;
      return dx * dx + dy * dy < (node.radius + 5) * (node.radius + 5);
    });

    if (clickedNodeIndex !== -1) {
      setDraggedNodeIndex(clickedNodeIndex);
      setSelectedNode(nodes[clickedNodeIndex]);
      nodes[clickedNodeIndex].fx = nodes[clickedNodeIndex].x;
      nodes[clickedNodeIndex].fy = nodes[clickedNodeIndex].y;
    } else {
      setIsDragging(true);
      dragStart.current = { x: e.clientX - transform.x, y: e.clientY - transform.y };
      setSelectedNode(null);
    }
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const clientX = e.clientX - rect.left;
    const clientY = e.clientY - rect.top;

    if (isDragging) {
      setTransform({
        ...transform,
        x: e.clientX - dragStart.current.x,
        y: e.clientY - dragStart.current.y,
      });
    } else if (draggedNodeIndex !== null) {
      // Update dragged node position
      const graphX = (clientX - transform.x) / transform.zoom;
      const graphY = (clientY - transform.y) / transform.zoom;
      nodes[draggedNodeIndex].fx = graphX;
      nodes[draggedNodeIndex].fy = graphY;
    }
  };

  const handleMouseUp = () => {
    setIsDragging(false);
    if (draggedNodeIndex !== null) {
      nodes[draggedNodeIndex].fx = null;
      nodes[draggedNodeIndex].fy = null;
      setDraggedNodeIndex(null);
    }
  };

  const zoom = (factor: number) => {
    setTransform((prev) => ({
      ...prev,
      zoom: Math.min(Math.max(prev.zoom * factor, 0.15), 4),
    }));
  };

  const resetView = () => {
    setTransform({ x: 100, y: 100, zoom: 0.95 });
  };

  const getPageDetail = () => {
    if (!selectedNode) return null;
    return (
      <div className="node-detail-panel glass-panel animate-fade-in">
        <h3 className="heading-md">{selectedNode.label}</h3>
        <span className="badge badge-purple">{selectedNode.type}</span>
        
        <p className="text-muted mt-2">
          {selectedNode.type === 'entity' && `Extracted entity matching class type "${selectedNode.subType}".`}
          {selectedNode.type === 'event' && `Synthesized event constructed from multiple claims.`}
          {selectedNode.type === 'document' && `Ingested source document.`}
        </p>

        <div className="node-actions mt-4">
          <button
            onClick={() => {
              const realId = selectedNode.id.split('-').slice(1).join('-');
              openDetail(selectedNode.type, realId);
            }}
            className="btn-primary"
          >
            Open Full Explorer
          </button>
        </div>
      </div>
    );
  };

  return (
    <div className="graph-container animate-fade-in">
      <div className="graph-controls-bar glass-card">
        {/* Search */}
        <div className="graph-search">
          <Search size={16} />
          <input
            type="text"
            placeholder="Search nodes..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="form-input"
          />
        </div>

        {/* Action buttons */}
        <div className="zoom-actions">
          <button onClick={() => zoom(1.25)} className="btn-secondary ctrl-btn" title="Zoom In">
            <ZoomIn size={16} />
          </button>
          <button onClick={() => zoom(0.8)} className="btn-secondary ctrl-btn" title="Zoom Out">
            <ZoomOut size={16} />
          </button>
          <button onClick={resetView} className="btn-secondary ctrl-btn" title="Recenter">
            <RotateCcw size={16} />
          </button>
        </div>
      </div>

      <div className="graph-workspace">
        <canvas
          ref={canvasRef}
          width={800}
          height={550}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseUp}
          className="graph-canvas glass-card"
        ></canvas>

        {/* Selected node details */}
        {getPageDetail()}
      </div>

      <div className="graph-legend glass-card">
        <div className="legend-title">Legend</div>
        <div className="legend-items">
          <div className="legend-item">
            <div className="legend-dot" style={{ background: '#3b82f6' }}></div>
            <span>Event</span>
          </div>
          <div className="legend-item">
            <div className="legend-dot" style={{ background: '#8b5cf6' }}></div>
            <span>Document</span>
          </div>
          <div className="legend-item">
            <div className="legend-dot" style={{ background: '#10b981' }}></div>
            <span>Location</span>
          </div>
          <div className="legend-item">
            <div className="legend-dot" style={{ background: '#06b6d4' }}></div>
            <span>Organization</span>
          </div>
          <div className="legend-item">
            <div className="legend-dot" style={{ background: '#ec4899' }}></div>
            <span>Concept</span>
          </div>
        </div>
      </div>

      <style>{`
        .graph-container {
          display: flex;
          flex-direction: column;
          gap: 1rem;
        }

        .graph-controls-bar {
          display: flex;
          justify-content: space-between;
          padding: 0.75rem 1rem !important;
          align-items: center;
        }

        .graph-search {
          display: flex;
          align-items: center;
          gap: 0.5rem;
          position: relative;
          width: 250px;
        }

        .graph-search svg {
          position: absolute;
          left: 0.75rem;
          color: var(--text-muted);
        }

        .graph-search input {
          padding-left: 2.25rem !important;
        }

        .zoom-actions {
          display: flex;
          gap: 0.5rem;
        }

        .ctrl-btn {
          padding: 0.5rem !important;
          border-radius: var(--radius-sm);
        }

        .graph-workspace {
          display: grid;
          grid-template-columns: 1fr auto;
          gap: 1rem;
          position: relative;
        }

        @media (max-width: 1024px) {
          .graph-workspace {
            grid-template-columns: 1fr;
          }
        }

        .graph-canvas {
          width: 100%;
          height: 550px;
          cursor: grab;
          border-radius: var(--radius-lg);
          background: rgba(0, 0, 0, 0.2);
        }

        .graph-canvas:active {
          cursor: grabbing;
        }

        .node-detail-panel {
          width: 280px;
          display: flex;
          flex-direction: column;
          gap: 1rem;
          text-align: left;
          height: 100%;
        }

        @media (max-width: 1024px) {
          .node-detail-panel {
            width: 100%;
            height: auto;
          }
        }

        .mt-2 { margin-top: 0.5rem; }
        .mt-4 { margin-top: 1rem; }

        .graph-legend {
          display: flex;
          align-items: center;
          gap: 1.5rem;
          padding: 0.75rem 1.5rem !important;
          font-size: 0.8rem;
          flex-wrap: wrap;
        }

        .legend-title {
          font-weight: 700;
          color: var(--text-secondary);
          text-transform: uppercase;
          letter-spacing: 0.05em;
        }

        .legend-items {
          display: flex;
          gap: 1.25rem;
          flex-wrap: wrap;
        }

        .legend-item {
          display: flex;
          align-items: center;
          gap: 0.4rem;
        }

        .legend-dot {
          width: 10px;
          height: 10px;
          border-radius: 50%;
        }
      `}</style>
    </div>
  );
};
