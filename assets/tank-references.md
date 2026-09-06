# Tank silhouette references

The game uses original simplified geometry in `src/game/models.ts`, inspired by these real vehicle families and online model references. No third-party model files or textures were imported.

- Skipper: M10 Booker-inspired welded angular turret, enclosed bustle, modular side skirts, modern sight housing and muzzle brake. Replaces the former T-72 dome and fuel drums while retaining Skipper’s compact game footprint. [US Army reference photographs](https://www.army.mil/article/275419/army_takes_delivery_of_first_m10_booker_combat_vehicle).
- Bruiser: M1A2 Abrams-style angular turret with long rear bustle, open stowage basket, seven wheels and smooth side skirts. [Abrams model](https://sketchfab.com/3d-models/m1a2-abrams-c85846177bfc4018b6a8f3b40754655c), [artist's rendered views](https://www.behance.net/gallery/37661357/M1A2-Abrams-Tank-3D-Model).
- Big Rig: Type 99-style pointed turret cheeks, segmented armor, six wheels and raised sight equipment. [Type 99 model and rendered views](https://www.turbosquid.com/3d-models/type-99-chinese-tank-3d-model-1253076).

These are stylized game classes, not accurate scale replicas or claims about real-world relative speed and armor. Stats remain unchanged. Hull collision bounds and projectile muzzles are measured from the same models used in play; selection previews and wreck parts also use those models.

Additional references discussed: [German Leopard 2](https://knds.com/en/products/leopard), [British Challenger 2](https://www.army.mod.uk/learn-and-explore/equipment/combat-vehicles/challenger-2/). Neither is a separate playable model in this pass.


## Proportions and common scale

Hull proportions include tracks and skirts. Targets are approximately 1.94 length/width for the compact Skipper and 2.17 for Abrams and Type 99. Model detail remains simplified. The fleet uses a common world scale, with comparable overall widths. Skipper retains its established game size after changing its visual reference to the M10 Booker; it is not an exact scale replica. Abrams is 1.95 game units wide; the other two are within 5% of that. The former 0.82/1.0/1.15 class multipliers exaggerated their size differences and are no longer used.

Dimension references: [T-72 manufacturer](https://avnl.co.in/products/cia-ajeya-t72), [Lithuanian Armed Forces magazine, Abrams hull 7.93 m / width 3.66 m](https://kariuomene.lt/data/public/uploads/2021/02/lmd_2020_nr.-3_kovas_internetui.pdf), [Type 99 dimensions](https://mil.news.sina.com.cn/2009-09-20/1246566604.html). The gun-forward length is distinct from hull length.

Projectile sweeps use the model hull plus a 0.18-unit shell-radius margin. Contact with tanks, walls and ground uses the unpadded model hull. Tests cover all chassis, rotated and mixed-class contact, nose/tail wall contact, and projectile hits/misses on each hull edge.
