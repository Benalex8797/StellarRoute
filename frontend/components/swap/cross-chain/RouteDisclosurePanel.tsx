'use client';

export function RouteDisclosurePanel() {
  return (
    <aside
      aria-label="Cross-chain risk disclosures"
      className="space-y-3 rounded-2xl border border-border/40 bg-muted/15 p-4 text-sm"
    >
      <h3 className="font-semibold text-foreground">Before you route</h3>
      <ul className="space-y-2 text-muted-foreground list-disc pl-5">
        <li>
          StellarRoute is non-custodial — you sign with your own wallets; we never
          hold keys.
        </li>
        <li>
          Cross-chain moves burn on the source chain before minting on the
          destination. Funds are not spendable on both sides during attestation.
        </li>
        <li>
          Attestation and finality times vary by corridor and network conditions.
        </li>
        <li>
          Only corridors marked executable can proceed to review and signing.
          Preview corridors show protocol steps without live quotes.
        </li>
      </ul>
    </aside>
  );
}
