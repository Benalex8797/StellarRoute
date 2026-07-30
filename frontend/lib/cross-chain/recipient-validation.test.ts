import { describe, expect, it } from 'vitest';
import { validateRecipientAddress } from './recipient-validation';

describe('validateRecipientAddress', () => {
  it('accepts Stellar G-address', () => {
    expect(
      validateRecipientAddress(
        'stellar',
        'GA5ZSEJYB37JRC5AVCIAZDL2Y343IFRMA2EO3HJWV2XG7H5V5CQRUP7W'
      ).valid
    ).toBe(true);
  });

  it('rejects invalid Stellar address', () => {
    expect(validateRecipientAddress('stellar', 'not-an-address').valid).toBe(
      false
    );
  });

  it('accepts EVM address', () => {
    expect(
      validateRecipientAddress('evm', '0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb0')
        .valid
    ).toBe(true);
  });

  it('accepts Solana address', () => {
    expect(
      validateRecipientAddress('solana', 'DYw8jCTfwHNRJhhmFcbXvVDTqWMEVFBX6ZKUmG5CNSKK')
        .valid
    ).toBe(true);
  });

  it('accepts Bitcoin testnet address', () => {
    expect(
      validateRecipientAddress('bitcoin', 'tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx')
        .valid
    ).toBe(true);
  });

  it('accepts TRON address', () => {
    expect(
      validateRecipientAddress('tron', 'TLyqzVGLV1srkB7dToTAEqgDSfPtXRJZYH').valid
    ).toBe(true);
  });
});
