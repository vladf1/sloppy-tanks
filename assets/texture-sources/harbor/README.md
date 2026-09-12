# Harbor material sources

Generated with the built-in image generation tool on 2026-09-11. Source images are stored here as quality-90 WebP at their generated 1254px resolution; runtime assets are resized to 512 square and encoded as WebP quality 80 in `public/textures/harbor/`.

## dock

Use case: photorealistic-natural. Asset type: seamless tileable game ground albedo texture, square 1024x1024. Straight-down orthographic scan of weathered light warm-gray concrete at a working container port. Fine exposed aggregate, subtle salt stains, broad soft mottling, small faded oil discolorations, sparse hairline cracks. Consistent low contrast, mid-light neutral gray, absolutely flat diffuse lighting, no shadows or highlights baked in, no perspective, no distinct borders or slabs, no objects, no paint markings, no text. Texture fills entire square edge to edge, intended to repeat over large ground plane beneath a stylized miniature tank game. Fine realistic microtexture but do not make a dark gritty or busy pattern.

## steel

Use case: photorealistic-natural. Asset type: seamless tileable square 1024x1024 game material albedo for painted steel, to multiply by colored ship and container paint. Flat orthographic close-up of pale warm-gray painted steel with subtle worn mottling, thin scratches, sparse tiny brown rust flecks and short downward rust streaks. Mostly intact pale paint, restrained weathering, no corrugation (geometry adds corrugation), no seams, no bolts, no edges, no words, no branding, no objects, no vignette, no baked lighting or shadows. Uniform diffuse illumination; tileable texture filling square edge to edge. Readable industrial miniature game material, not ruined grunge.


## Runtime encoding

The two WebPs total about 87 KiB. The 1254px WebP editing sources total about 941 KiB and are not shipped by Vite. The original generated PNGs remain outside the repository in the image-generation output folder. Each runtime image uses 512px per side with mipmaps; compared with 1024px this cuts decoded texture storage by 75%.

```sh
cwebp -q 80 -resize 512 512 -metadata none assets/texture-sources/harbor/dock.webp -o public/textures/harbor/dock.webp
cwebp -q 80 -resize 512 512 -metadata none assets/texture-sources/harbor/steel.webp -o public/textures/harbor/steel.webp
```
