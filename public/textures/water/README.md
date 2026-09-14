# Water normal map

`normals.webp` is the official Three.js Water example's `waternormals.jpg`, resized
to 512 × 512 and encoded as WebP quality 90. Both maps share this texture. It is
normal data, so it is sampled without an sRGB color-space conversion.

Source: https://github.com/mrdoob/three.js/blob/r185/examples/textures/waternormals.jpg

License: Three.js MIT license, included in `LICENSE.txt`.

The game imports `three/addons/objects/Water.js` from the existing pinned Three.js
dependency. No separate runtime package or remote asset request is needed.
