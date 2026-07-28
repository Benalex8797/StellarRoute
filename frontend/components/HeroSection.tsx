'use client';

import { useMemo } from 'react';
import { Button } from '@/components/ui/button';
import { ArrowRight } from 'lucide-react';
import Link from 'next/link';
import { cn } from '@/lib/utils';
import { useReducedMotion } from '@/hooks/useReducedMotion';

export function HeroSection() {
  const featuredPair = {
    from: 'native',
    to: 'USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
    amount: '100',
  };

  const prefersReducedMotion = useReducedMotion();

  const swapUrl = useMemo(
    () =>
      `/swap?from=${encodeURIComponent(featuredPair.from)}&to=${encodeURIComponent(featuredPair.to)}&amount=${featuredPair.amount}&ts=${Date.now()}`,
    [featuredPair.from, featuredPair.to, featuredPair.amount]
  );

  return (
    <section className="relative min-h-[calc(100vh-8rem)] overflow-hidden">
      {/* Chart atmosphere layers — keep testids for reduced-motion suite */}
      <div className="absolute inset-0 -z-10">
        <div
          data-testid="hero-gradient-1"
          className={cn(
            'absolute -left-10 top-16 h-[28rem] w-[28rem] rounded-[40%] bg-primary/15',
            !prefersReducedMotion && 'animate-pulse'
          )}
        />
        <div
          data-testid="hero-gradient-2"
          className={cn(
            'absolute -right-16 bottom-10 h-[24rem] w-[24rem] rounded-[35%] bg-signal/15',
            !prefersReducedMotion && 'animate-pulse delay-700'
          )}
        />
        <div
          className={cn(
            'absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/50 to-transparent',
            !prefersReducedMotion && 'animate-in fade-in duration-700'
          )}
        />
      </div>

      <div className="container mx-auto flex min-h-[calc(100vh-8rem)] flex-col justify-center px-4 py-16 sm:px-6 lg:px-8">
        <div className="max-w-3xl space-y-8">
          <p
            className={cn(
              'brand-wordmark text-5xl leading-none text-foreground sm:text-7xl lg:text-8xl',
              !prefersReducedMotion && 'animate-in fade-in slide-in-from-bottom-3 duration-700'
            )}
          >
            StellarRoute
          </p>

          <h1
            className={cn(
              'max-w-xl text-xl font-medium tracking-tight text-foreground/90 sm:text-2xl',
              !prefersReducedMotion && 'animate-in fade-in slide-in-from-bottom-3 duration-700 delay-150'
            )}
          >
            Chart the best path across SDEX and Soroban AMMs.
          </h1>

          <p
            className={cn(
              'max-w-lg text-base text-muted-foreground sm:text-lg',
              !prefersReducedMotion && 'animate-in fade-in slide-in-from-bottom-3 duration-700 delay-300'
            )}
          >
            One route deck for Stellar liquidity — sharper fills, clearer
            venue signal, less guesswork before you sign.
          </p>

          <div
            className={cn(
              'flex flex-col gap-3 pt-2 sm:flex-row sm:items-center',
              !prefersReducedMotion && 'animate-in fade-in slide-in-from-bottom-3 duration-700 delay-500'
            )}
          >
            <Button
              asChild
              size="lg"
              className="h-12 min-h-11 rounded-lg px-7 text-base font-semibold"
            >
              <Link href={swapUrl}>
                Start with XLM → USDC
                <ArrowRight className="ml-2 h-5 w-5" />
              </Link>
            </Button>

            <Button
              asChild
              variant="outline"
              size="lg"
              className="h-12 min-h-11 rounded-lg border-border/70 bg-background/40 px-7 text-base font-semibold backdrop-blur-sm"
            >
              <Link href="/swap">Open swap deck</Link>
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}
