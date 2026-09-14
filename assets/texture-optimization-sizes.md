# Large texture optimization

Sizes in decimal KB. Original inputs are preserved outside the deployed public directory.

| Texture            | Before dimensions | After dimensions | Quality | Before KB | After KB | Reduction |
| ------------------ | ----------------- | ---------------- | ------: | --------: | -------: | --------: |
| dry-grass          | 1024×1024         | 768×768          |      75 |    370.51 |   203.01 |     45.2% |
| packed-dirt        | 1024×1024         | 512×512          |      80 |    198.23 |    56.43 |     71.5% |
| weathered-concrete | 1024×1024         | 512×512          |      80 |    155.36 |    36.66 |     76.4% |
| sandstone          | 1024×1024         | 768×768          |      75 |    303.90 |    90.64 |     70.2% |

Total: 1028008 → 386740 bytes; 641268 bytes saved (62.4%).

Compared 1024px quality 65, 768px quality 75, and 512px quality 80 candidates. Kept grass and sandstone at 768px to retain fine detail; dirt and concrete at 512px. These are lossy reductions, not pixel-identical conversions. Checked the contact sheet and live Pine Village and Dusty Dig rendering in the built-in browser. No cold-load timing benchmark was run; these are asset-size savings, not measured load-time savings.

The sandstone source was already lossy. Its pre-optimization file is now preserved under assets/texture-sources/quarry to prevent cumulative re-encoding.

Local candidate comparison: artifacts/performance/texture-optimization/comparison.png.
