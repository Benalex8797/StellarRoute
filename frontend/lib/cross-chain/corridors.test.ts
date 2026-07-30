import { describe, expect, it } from 'vitest';
import {
  CORRIDOR_CATALOG,
  catalogMatchesBackendRoutes,
  chainFamilyForDisplayId,
  isCorridorExecutable,
  resolveCorridorAvailability,
} from './corridors';
import { hasBackendRoute } from '@/lib/wallet/adapters';

describe('corridor catalog', () => {
  it('keeps catalog availability synchronized with hasBackendRoute', () => {
    expect(catalogMatchesBackendRoutes()).toBe(true);
  });

  it('only marks stellar-native as executable', () => {
    const executable = CORRIDOR_CATALOG.filter((c) => isCorridorExecutable(c));
    expect(executable).toHaveLength(1);
    expect(executable[0].id).toBe('stellar-native');
  });

  it('reflects backend route registration per corridor leg', () => {
    for (const corridor of CORRIDOR_CATALOG) {
      const source = chainFamilyForDisplayId(corridor.sourceChainId);
      const dest = chainFamilyForDisplayId(corridor.destChainId);
      const backend = hasBackendRoute(source, dest);
      const availability = resolveCorridorAvailability(corridor);
      if (backend) {
        expect(availability).toBe('executable');
      } else {
        expect(availability).toBe('coming_soon');
      }
    }
  });
});
