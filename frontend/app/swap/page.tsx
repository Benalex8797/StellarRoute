import { SwapPageClient } from "./SwapPageClient";
import { Metadata } from "next";

export const metadata: Metadata = {
  title: "Swap Tokens | StellarRoute",
  description: "Swap assets on Stellar with the best rates and lowest slippage across all DEXs and AMMs.",
  openGraph: {
    title: "Swap Tokens | StellarRoute",
    description: "Best-price routing across Stellar DEX and Soroban AMM pools.",
    type: "website",
    url: "https://stellarroute.app/swap",
    images: [
      {
        url: "/icons/icon-512.svg",
        width: 512,
        height: 512,
        alt: "StellarRoute swap interface preview",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    title: "Swap Tokens | StellarRoute",
    description: "Swap assets on Stellar with the best rates and lowest slippage across all DEXs and AMMs.",
    images: ["/icons/icon-512.svg"],
  },
};

export default function SwapPage() {
  return (
    <div className="mx-auto w-full max-w-5xl py-2 sm:py-4">
      <SwapPageClient />
      <div className="mt-10 flex flex-wrap justify-center gap-6 text-muted-foreground sm:gap-8">
        <div className="flex items-center gap-2">
          <div className="h-1.5 w-1.5 rounded-full bg-success" />
          <span className="font-mono text-[10px] font-semibold uppercase tracking-[0.2em]">
            Horizon live
          </span>
        </div>
        <div className="flex items-center gap-2">
          <div className="h-1.5 w-1.5 rounded-full bg-chart-3" />
          <span className="font-mono text-[10px] font-semibold uppercase tracking-[0.2em]">
            Soroban ready
          </span>
        </div>
        <div className="flex items-center gap-2">
          <div className="h-1.5 w-1.5 rounded-full bg-primary" />
          <span className="font-mono text-[10px] font-semibold uppercase tracking-[0.2em]">
            Best execution
          </span>
        </div>
      </div>
    </div>
  );
}
