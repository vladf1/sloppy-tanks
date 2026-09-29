// Batched debris fade (`debris-fade.ts`). The color pass already multiplies the
// material alpha by the instance opacity; the effect's `shadow_fade` flag gives
// its shadow pass the stable spatial dither (`tsl_hash` of 100 * dot(pw, pw)),
// so the shadow thins with the fade without a translucent shadow pass.
