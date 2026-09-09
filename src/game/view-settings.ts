/** Camera distances are world metres; animation durations are seconds. */
export const CAMERA = {
  fieldOfView: 43,
  near: 0.1,
  far: 320,
  defaultZoom: 34,
  minZoom: 23,
  maxZoom: 52,
  maxPixelRatio: 1.5,
} as const;
export const FEEDBACK = {
  hitConfirmationSeconds: 0.16,
  recoilSeconds: 0.28,
  spawnCueSeconds: 2.5,
  spawnPulseSeconds: 1.25,
  pickupSeconds: 0.8,
  maxPickupEffects: 24,
  flashDecay: 12,
} as const;
