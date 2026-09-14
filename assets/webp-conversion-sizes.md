# PNG to WebP conversion

Lossless conversion; all 22 images passed decoded pixel and dimension comparison after a WebP round trip. Sizes are file bytes (KiB = 1024 bytes). Source artwork is not deployed.

| Asset                                          | PNG KiB | WebP KiB | Reduction |
| ---------------------------------------------- | ------: | -------: | --------: |
| public/textures/barrels/painted-drum.png       |   77.46 |    43.90 |     43.3% |
| public/textures/houses/shingles.png            |    3.04 |     0.70 |     76.9% |
| public/textures/houses/siding.png              |    9.39 |     3.29 |     64.9% |
| public/textures/pickups/laser.png              |    7.94 |     3.06 |     61.5% |
| public/textures/pickups/piercing.png           |    3.89 |     1.19 |     69.4% |
| public/textures/pickups/rapid.png              |    4.24 |     1.53 |     63.9% |
| public/textures/pickups/repair.png             |    3.05 |     0.77 |     74.9% |
| public/textures/pickups/ricochet.png           |    7.71 |     3.09 |     60.0% |
| public/textures/pickups/rocket.png             |    4.99 |     1.61 |     67.7% |
| public/textures/pickups/shield.png             |    7.37 |     2.76 |     62.6% |
| public/textures/pickups/speed.png              |    5.00 |     1.99 |     60.2% |
| public/textures/pickups/spread.png             |    7.74 |     3.24 |     58.1% |
| public/textures/trees/birch.png                |   10.35 |     4.20 |     59.4% |
| public/textures/trees/leaves.png               |   41.45 |    19.79 |     52.3% |
| public/textures/trees/needles.png              |   33.94 |    14.61 |     57.0% |
| public/textures/trees/rings.png                |   72.24 |    26.92 |     62.7% |
| assets/texture-sources/dry-grass.png           | 3641.14 |  2608.45 |     28.4% |
| assets/texture-sources/packed-dirt.png         | 3120.31 |  2228.62 |     28.6% |
| assets/texture-sources/tanks/armor-wear.png    |  235.87 |   124.91 |     47.0% |
| assets/texture-sources/trees/bark-original.png | 3292.63 |  2333.36 |     29.1% |
| assets/texture-sources/trees/conifer-spray.png |  246.78 |   138.33 |     43.9% |
| assets/texture-sources/weathered-concrete.png  | 3086.80 |  2018.97 |     34.6% |

Runtime: 306,984 → 135,820 bytes (55.8% smaller).

Source artwork: 13,950,487 → 9,679,496 bytes (30.6% smaller).

Total: 14,257,471 → 9,815,316 bytes (31.2% smaller).
