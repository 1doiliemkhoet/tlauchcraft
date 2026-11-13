import { describe, expect, it } from 'vitest';
import { BackendUnavailable, EMPTY_CAPABILITIES, requireFeature, safeArchive, safePercent } from './backend';
describe('capability guard', () => {
  it('denies commands without implemented backend', () => expect(() => requireFeature(EMPTY_CAPABILITIES, 'game')).toThrow(BackendUnavailable));
  it('accepts only negotiated feature', () => {
    const caps = { ...EMPTY_CAPABILITIES, features: { ...EMPTY_CAPABILITIES.features, profiles: true } };
    expect(() => requireFeature(caps, 'profiles')).not.toThrow();
    expect(() => requireFeature(caps, 'lan')).toThrow();
  });
});
describe('UX input validation', () => {
  it('rejects files without supported extension', () => { expect(safeArchive('pack.zip')).toBe(true); expect(safeArchive('pack.mrpack')).toBe(true); expect(safeArchive('pack.exe')).toBe(false); expect(safeArchive('')).toBe(false); });
  it('clamps out of range percentages', () => { expect(safePercent(-50)).toBe(0); expect(safePercent(135)).toBe(100); expect(safePercent(NaN)).toBeUndefined(); });
});
