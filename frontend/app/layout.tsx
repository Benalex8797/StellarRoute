import type { Metadata } from "next";
import { Bricolage_Grotesque, Figtree, JetBrains_Mono } from "next/font/google";
import "./globals.css";
import { Providers } from "./providers";
import { Toaster } from "@/components/ui/sonner";
import { AppShell } from "@/components/layout/app-shell";
import ErrorBoundary from "../components/ErrorBoundary";

const display = Bricolage_Grotesque({
  variable: "--font-bricolage",
  subsets: ["latin"],
  weight: ["500", "600", "700", "800"],
});

const sans = Figtree({
  variable: "--font-figtree",
  subsets: ["latin"],
  weight: ["400", "500", "600", "700"],
});

const mono = JetBrains_Mono({
  variable: "--font-jetbrains",
  subsets: ["latin"],
  weight: ["400", "500", "600"],
});

export const metadata: Metadata = {
  title: "StellarRoute - DEX Aggregator for Stellar",
  description: "Best-price routing across Stellar DEX and Soroban AMM pools",

  manifest: "/manifest.json",
  themeColor: "#060b11",

  icons: {
    icon: "/icons/icon-192.svg",
    apple: "/icons/icon-192.svg",
  },

  appleWebApp: {
    capable: true,
    statusBarStyle: "default",
    title: "StellarRoute",
  },

  openGraph: {
    title: "StellarRoute - DEX Aggregator for Stellar",
    description: "Best-price routing across Stellar DEX and Soroban AMM pools",
    url: "https://stellarroute.app",
    type: "website",
    images: [
      {
        url: "/icons/icon-512.svg",
        width: 512,
        height: 512,
        alt: "StellarRoute logo",
      },
    ],
  },

  twitter: {
    card: "summary_large_image",
    title: "StellarRoute - DEX Aggregator for Stellar",
    description: "Best-price routing across Stellar DEX and Soroban AMM pools",
    images: ["/icons/icon-512.svg"],
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body
        className={`${display.variable} ${sans.variable} ${mono.variable} antialiased min-h-screen flex flex-col`}
      >
        <ErrorBoundary>
          <Providers>
            <AppShell>{children}</AppShell>
          </Providers>
        </ErrorBoundary>

        <Toaster position="top-right" richColors />
      </body>
    </html>
  );
}
