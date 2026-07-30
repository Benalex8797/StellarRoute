import { useMemo } from 'react';
import type { CorridorDefinition, CorridorId } from '@/lib/cross-chain/types';
import {
  CORRIDOR_CATALOG,
  findCorridorById,
  isCorridorExecutable,
  resolveCorridorAvailability,
} from '@/lib/cross-chain/corridors';

export function useCorridorCatalog() {
  const corridors = useMemo(
    () =>
      CORRIDOR_CATALOG.map((corridor) => ({
        ...corridor,
        availability: resolveCorridorAvailability(corridor),
        executable: isCorridorExecutable(corridor),
      })),
    []
  );

  const executableCorridors = useMemo(
    () => corridors.filter((c) => c.executable),
    [corridors]
  );

  return {
    corridors,
    executableCorridors,
    getCorridor: (id: CorridorId) => {
      const base = findCorridorById(id);
      return {
        ...base,
        availability: resolveCorridorAvailability(base),
        executable: isCorridorExecutable(base),
      };
    },
  };
}

export type EnrichedCorridor = CorridorDefinition & {
  availability: 'executable' | 'coming_soon';
  executable: boolean;
};
