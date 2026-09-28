/** Camera distances are world metres; animation durations are seconds. */
export const CAMERA = {
  fieldOfView: 43,
  near: 0.1,
  far: 320,
  defaultZoom: 34,
  minZoom: 17,
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
/** World-space HUD (reticle, tank bars) draws only for the main camera; water
 * reflections disable this layer on their mirror camera so they never show it. */
export const HUD_LAYER = 1;
/** The camera mounted on the player's turret. The eye sits in turret-local
 * model units (before vehicle scale): behind the mantlet, raised like a
 * commander standing in the hatch, so the gun and front deck stay in view and
 * other tanks show their roofs instead of a flat side-on silhouette. */
export const FIRST_PERSON = {
  fieldOfView: 58,
  eye: {
    scout: { height: 2.22, forward: -0.55 },
    balanced: { height: 2.32, forward: -0.75 },
    heavy: { height: 2.36, forward: -0.7 },
    humvee: { height: 2.45, forward: -0.45 },
  },
  // A slight downward tilt shows the ground ahead without hiding the horizon.
  pitch: -0.07,
  mouseRadiansPerPixel: 0.0032,
  touchTurnRadiansPerSecond: 2.4,
  // The reticle floats this far from the eye, drawn over the scene; its
  // scale keeps it about as large on screen as the overhead ground reticle.
  reticleDistance: 12,
  reticleScale: 0.3,
  // Metres along the shell lane where the reticle sits: a typical engagement.
  reticleRange: 25,
  // Pickups hover at eye height; fading them keeps tanks behind them visible.
  pickupOpacity: 0.7,
  // Switching views flies the camera between the overhead pose and the eye.
  // It keeps the overhead gaze on the tank until this fraction of the flight,
  // then swings to the turret's heading.
  transitionSeconds: 0.8,
  transitionTurnStart: 0.4,
} as const;
