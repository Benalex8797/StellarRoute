import { Metadata } from 'next';
import { OfframpPageClient } from './OfframpPageClient';

export const metadata: Metadata = {
  title: 'Offramp to Naira | StellarRoute',
  description:
    'Cash out Stellar USDC — or bridge any supported stablecoin — to Nigerian Naira.',
  openGraph: {
    title: 'Offramp to Naira | StellarRoute',
    description:
      'Stablecoin to local fiat: direct Stellar USDC or bridge-then-offramp to ₦ Naira.',
    type: 'website',
    url: 'https://stellarroute.app/offramp',
    images: [
      {
        url: '/icons/icon-512.svg',
        width: 512,
        height: 512,
        alt: 'StellarRoute offramp',
      },
    ],
  },
  twitter: {
    card: 'summary_large_image',
    title: 'Offramp to Naira | StellarRoute',
    description:
      'Cash out Stellar USDC or bridge another coin into Naira via Stellar.',
    images: ['/icons/icon-512.svg'],
  },
};

export default function OfframpPage() {
  return (
    <div className="mx-auto w-full max-w-5xl py-2 sm:py-4">
      <OfframpPageClient />
    </div>
  );
}
