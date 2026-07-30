'use client';

import { cn } from '@/lib/utils';
import type { CorridorId, CorridorSelectionId } from '@/lib/cross-chain/types';
import { UNMATCHED_CORRIDOR_ID } from '@/lib/cross-chain/corridors';
import { useCorridorCatalog } from '@/hooks/useCorridorCatalog';

interface CorridorTabsProps {
  activeId: CorridorSelectionId;
  onSelect: (id: CorridorId) => void;
  disabled?: boolean;
}

export function CorridorTabs({ activeId, onSelect, disabled }: CorridorTabsProps) {
  const { corridors } = useCorridorCatalog();
  const isUnmatched = activeId === UNMATCHED_CORRIDOR_ID;

  return (
    <nav aria-label="Cross-chain corridors" className="space-y-2">
      <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
        Corridor catalog
      </p>
      <div
        role="tablist"
        className="flex flex-wrap gap-2"
        aria-orientation="horizontal"
      >
        {corridors.map((corridor) => {
          const selected = !isUnmatched && corridor.id === activeId;
          return (
            <button
              key={corridor.id}
              type="button"
              role="tab"
              id={`corridor-tab-${corridor.id}`}
              aria-selected={selected}
              aria-controls={`corridor-panel-${corridor.id}`}
              onClick={() => onSelect(corridor.id)}
              disabled={disabled}
              className={cn(
                'min-h-11 rounded-xl border px-3 py-2 text-left transition-colors',
                'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                selected
                  ? 'border-primary/50 bg-primary/12'
                  : 'border-border/50 bg-background/40 hover:bg-muted/30',
                !corridor.executable && 'opacity-90'
              )}
              data-testid={`corridor-tab-${corridor.id}`}
            >
              <span className="block text-xs font-semibold">{corridor.label}</span>
              <span className="block text-[10px] text-muted-foreground">
                {corridor.executable ? 'Executable' : 'Coming soon'}
              </span>
            </button>
          );
        })}
      </div>
      {isUnmatched && (
        <p className="text-xs text-muted-foreground" id="corridor-tab-unmatched">
          Custom chain pair — not in catalog
        </p>
      )}
    </nav>
  );
}
