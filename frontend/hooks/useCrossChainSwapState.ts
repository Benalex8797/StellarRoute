'use client';

import { useCallback, useMemo, useState } from 'react';
import {
  CHAIN_DEFINITIONS,
  findCorridorForChains,
  isCorridorExecutable,
  resolveCorridorAvailability,
  chainFamilyForDisplayId,
} from '@/lib/cross-chain/corridors';
import { validateRecipientAddress } from '@/lib/cross-chain/recipient-validation';
import type {
  ChainDisplayId,
  CorridorId,
  ExecutionTimelineStep,
} from '@/lib/cross-chain/types';

export type CrossChainSwapStoryFixture =
  | 'live'
  | 'stellar-native'
  | 'evm-to-stellar'
  | 'wallets-partial'
  | 'network-mismatch'
  | 'unsupported-corridor'
  | 'executing-timeline';

const FIXTURE_CHAINS: Partial<
  Record<CrossChainSwapStoryFixture, { source: ChainDisplayId; dest: ChainDisplayId }>
> = {
  'stellar-native': { source: 'stellar', dest: 'stellar' },
  'evm-to-stellar': { source: 'ethereum-sepolia', dest: 'stellar' },
  'unsupported-corridor': { source: 'solana', dest: 'stellar' },
};

export function useCrossChainSwapState(options?: {
  storyFixture?: CrossChainSwapStoryFixture;
}) {
  const fixture = options?.storyFixture ?? 'live';
  const fixtureChains = FIXTURE_CHAINS[fixture];

  const [corridorId, setCorridorId] = useState<CorridorId>(
    fixture === 'evm-to-stellar' ? 'evm-to-stellar' : 'stellar-native'
  );
  const [sourceChainId, setSourceChainId] = useState<ChainDisplayId>(
    fixtureChains?.source ?? 'stellar'
  );
  const [destChainId, setDestChainId] = useState<ChainDisplayId>(
    fixtureChains?.dest ?? 'stellar'
  );
  const [sourceAmount, setSourceAmount] = useState('');
  const [recipientOverride, setRecipientOverride] = useState('');
  const [useRecipientOverride, setUseRecipientOverride] = useState(false);

  const corridor = useMemo(() => {
    const match = findCorridorForChains(sourceChainId, destChainId);
    if (match) return match;
    return findCorridorForChains('stellar', 'stellar')!;
  }, [sourceChainId, destChainId]);

  const availability = resolveCorridorAvailability(corridor);
  const executable = isCorridorExecutable(corridor);
  const isStellarNative =
    sourceChainId === 'stellar' && destChainId === 'stellar';

  const destChainFamily = chainFamilyForDisplayId(destChainId);
  const recipientValidation = useMemo(() => {
    if (!useRecipientOverride || !recipientOverride.trim()) {
      return { valid: true as const };
    }
    return validateRecipientAddress(destChainFamily, recipientOverride);
  }, [useRecipientOverride, recipientOverride, destChainFamily]);

  const canReview = executable && recipientValidation.valid;

  const selectCorridor = useCallback((id: CorridorId) => {
    setCorridorId(id);
    const fromCatalog = CORRIDOR_CATALOG_ENTRY(id);
    setSourceChainId(fromCatalog.sourceChainId);
    setDestChainId(fromCatalog.destChainId);
  }, []);

  const selectSourceChain = useCallback((id: ChainDisplayId) => {
    setSourceChainId(id);
    const match = findCorridorForChains(id, destChainId);
    if (match) setCorridorId(match.id);
  }, [destChainId]);

  const selectDestChain = useCallback((id: ChainDisplayId) => {
    setDestChainId(id);
    const match = findCorridorForChains(sourceChainId, id);
    if (match) setCorridorId(match.id);
  }, [sourceChainId]);

  const timelineSteps: ExecutionTimelineStep[] = useMemo(() => {
    if (fixture === 'executing-timeline') {
      return EXECUTING_TIMELINE_FIXTURE;
    }
    if (!executable) {
      return PREVIEW_TIMELINE_UNAVAILABLE;
    }
    if (isStellarNative) {
      return STELLAR_NATIVE_TIMELINE_IDLE;
    }
    return PREVIEW_TIMELINE_UNAVAILABLE;
  }, [executable, isStellarNative, fixture]);

  return {
    fixture,
    corridorId,
    corridor,
    availability,
    executable,
    isStellarNative,
    sourceChainId,
    destChainId,
    sourceChain: CHAIN_DEFINITIONS[sourceChainId],
    destChain: CHAIN_DEFINITIONS[destChainId],
    sourceAmount,
    setSourceAmount,
    recipientOverride,
    setRecipientOverride,
    useRecipientOverride,
    setUseRecipientOverride,
    recipientValidation,
    canReview,
    selectCorridor,
    selectSourceChain,
    selectDestChain,
    timelineSteps,
  };
}

function CORRIDOR_CATALOG_ENTRY(id: CorridorId) {
  const entries = {
    'stellar-native': {
      sourceChainId: 'stellar' as ChainDisplayId,
      destChainId: 'stellar' as ChainDisplayId,
    },
    'evm-to-stellar': {
      sourceChainId: 'ethereum-sepolia' as ChainDisplayId,
      destChainId: 'stellar' as ChainDisplayId,
    },
    'stellar-to-evm': {
      sourceChainId: 'stellar' as ChainDisplayId,
      destChainId: 'ethereum-sepolia' as ChainDisplayId,
    },
    'solana-to-stellar': {
      sourceChainId: 'solana' as ChainDisplayId,
      destChainId: 'stellar' as ChainDisplayId,
    },
    'bitcoin-to-stellar': {
      sourceChainId: 'bitcoin' as ChainDisplayId,
      destChainId: 'stellar' as ChainDisplayId,
    },
    'tron-to-stellar': {
      sourceChainId: 'tron' as ChainDisplayId,
      destChainId: 'stellar' as ChainDisplayId,
    },
  };
  return entries[id];
}

const STELLAR_NATIVE_TIMELINE_IDLE: ExecutionTimelineStep[] = [
  {
    id: 'stellar_swap',
    label: 'Stellar swap',
    description: 'Sign and submit via Stellar wallet when you review.',
    status: 'pending',
  },
  {
    id: 'burn',
    label: 'Burn',
    description: 'Not used for same-chain Stellar swaps.',
    status: 'unavailable',
  },
  {
    id: 'attest',
    label: 'Attest',
    description: 'Not used for same-chain Stellar swaps.',
    status: 'unavailable',
  },
  {
    id: 'mint',
    label: 'Mint',
    description: 'Not used for same-chain Stellar swaps.',
    status: 'unavailable',
  },
];

const PREVIEW_TIMELINE_UNAVAILABLE: ExecutionTimelineStep[] = [
  {
    id: 'stellar_swap',
    label: 'Stellar swap',
    description: 'Available when this corridor is executable.',
    status: 'unavailable',
  },
  {
    id: 'burn',
    label: 'Burn',
    description: 'Source-chain burn — protocol preview only.',
    status: 'unavailable',
  },
  {
    id: 'attest',
    label: 'Attest',
    description: 'Circle attestation — timing varies by corridor.',
    status: 'unavailable',
  },
  {
    id: 'mint',
    label: 'Mint',
    description: 'Destination mint after attestation.',
    status: 'unavailable',
  },
];

const EXECUTING_TIMELINE_FIXTURE: ExecutionTimelineStep[] = [
  {
    id: 'stellar_swap',
    label: 'Stellar swap',
    description: 'Fixture: prior step completed in story preview.',
    status: 'complete',
    href: 'https://stellar.expert/explorer/testnet/tx/fixture-stellar',
    supportReference: 'SR-FIXTURE-001',
  },
  {
    id: 'burn',
    label: 'Burn',
    description: 'Fixture: burn submitted — awaiting attestation.',
    status: 'active',
    href: 'https://sepolia.etherscan.io/tx/fixture-burn',
    supportReference: 'SR-FIXTURE-002',
    retryable: true,
  },
  {
    id: 'attest',
    label: 'Attest',
    description: 'Fixture: attestation pending.',
    status: 'pending',
  },
  {
    id: 'mint',
    label: 'Mint',
    description: 'Fixture: mint not started.',
    status: 'unavailable',
  },
];
