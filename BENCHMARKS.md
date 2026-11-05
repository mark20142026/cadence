# Benchmarks: Cadence vs vanilla Geometry Dash 2.2091

2026-11-04, MacBook Air M2 (8 GB), macOS 27. Both at 1280×720 pixels (640×360 points at 2×), 20 s from each level's start with noclip (deaths skipped the same way in both), one program at a time. Frame times are wall-clock intervals between presented frames; CPU is CPU time per second of play (100% = one core); MB is the process footprint. Vanilla is the real game (the harness copy, Geode bench mode), Cadence the release build with CADENCE_LEVEL_FILE.

```
level                             FPS    p50    p99    max >20ms  CPU/s     MB
Sonic Wave Infinity    vanilla   59.9   16.7   19.2   33.3     5    23%    998
                       Cadence   60.0   16.7   19.6   33.8     9    32%    593
Tidal Wave             vanilla   60.0   16.7   19.2   30.6     3    22%    985
                       Cadence   60.0   16.7   19.6   32.6     9    33%    491
Zodiac                 vanilla   59.9   16.7   19.3   32.4     2    22%    798
                       Cadence   60.0   16.7   19.7   22.5    13    32%    456
Bloodlust              vanilla   60.0   16.6   20.3   21.5    18    22%    710
                       Cadence   60.0   16.7   19.7   21.2    10    34%    410
Firework               vanilla   60.0   16.7   20.5   32.5    25    24%    729
                       Cadence   60.0   16.7   19.1   33.8     2    33%    432
Avernus                vanilla   59.9   16.7   19.6   50.5     5    24%    739
                       Cadence   60.0   16.7   19.1   21.2     3    35%    437
Kenos                  vanilla   59.9   16.7   20.5   32.7    19    22%    730
                       Cadence   60.0   16.7   19.5   23.2     7    34%    417
Slaughterhouse         vanilla   59.9   16.6   21.0   48.1    49    23%    678
                       Cadence   60.0   16.7   19.4   20.3     3    32%    418
Abyss of Darkness      vanilla   60.0   16.7   19.2   32.5     2    23%    632
                       Cadence   60.0   16.7   19.8   22.0    11    33%    401
Tartarus               vanilla   59.9   16.7   19.0   34.2     1    18%    422
                       Cadence   60.0   16.7   20.8   33.1    17    19%    339
Acu                    vanilla   59.9   16.7   18.9   32.8     1    22%    416
                       Cadence   60.0   16.7   20.1   33.1    14    32%    344
pc killer              vanilla   57.7   16.7   35.3   37.3    52    37%    816
                       Cadence   59.5   16.7   33.1   35.1    39    41%    732
Lag Machine            vanilla   49.7   16.9   51.7   54.3   140    42%    714
                       Cadence   57.4   16.7   34.0   50.6   135    40%    765
stress test            vanilla   59.8   16.6   22.5   32.5    62    26%    558
                       Cadence   60.0   16.7   19.2   21.8     8    34%    354
Bloodbath              vanilla   60.0   16.7   19.1   31.5     1    18%    385
                       Cadence   60.0   16.7   20.7   21.6    19    18%    327
```

Levels are in bench-levels/ (not in the repository). Reproduce: harness/run-trace.sh with `levelstring`/`bench`/`noclip` for vanilla; `CADENCE_WINDOW=640x360 CADENCE_NOCLIP=1 CADENCE_BENCH=22 CADENCE_BENCH_OUT=… CADENCE_LEVEL_FILE=bench-levels/<id>.txt cadence-game <id>` for Cadence.

## While the Mac is in use (2026-11-04 evening)

Same levels, 20 s each, noclip, 1280×720 pixels, one program at a time, but run with Mark working
on the laptop, through `tools/bench_pair.py`. Both windows were parked on top past the screen's
bottom-right corner (a 320×200-point piece showing), never focused (no run came to the front),
music playing muted in both. CPU and MB are the kernel's counters for the whole process (CPU
time / wall time since launch, lifetime peak footprint). "Cadence" is the cache off
(`CADENCE_NOCACHE`), "+ cache" the static draw cache on.

```
level                  mode        FPS    p50    p99    max >20ms  CPU/s     MB
Sonic Wave Infinity    vanilla    60.0   16.7   18.4   31.6     1    33%   1045
                       Cadence    59.8   16.7   17.5  111.0     1    24%    723
                       + cache    60.0   16.7   17.7   20.7     1    25%    720
Tidal Wave             vanilla    60.0   16.6   18.8   20.4     2    42%   1032
                       Cadence    60.0   16.7   17.5   24.9     1    31%    650
                       + cache    60.0   16.7   17.5   24.0     1    31%    669
Zodiac                 vanilla    60.0   16.7   18.5   31.8     2    41%    821
                       Cadence    60.0   16.7   17.5   18.6     0    31%    583
                       + cache    60.0   16.7   17.4   17.8     0    31%    601
Bloodlust              vanilla    59.2   16.7   32.0   34.8   102    42%    710
                       Cadence    60.0   16.7   17.6   22.2     1    28%    559
                       + cache    60.0   16.7   17.6   18.2     0    28%    571
Firework               vanilla    58.7   16.7   33.3   35.5   125    40%    753
                       Cadence    60.0   16.7   17.7   20.2     1    31%    598
                       + cache    60.0   16.7   17.6   18.4     0    31%    595
Avernus                vanilla    58.6   16.6   33.2   38.0   165    42%    752
                       Cadence    60.0   16.7   17.6   18.9     0    31%    575
                       + cache    59.9   16.7   17.8   33.9     2    31%    595
Kenos                  vanilla    59.7   16.6   22.4   36.3    50    39%    714
                       Cadence    60.0   16.7   17.6   18.0     0    31%    545
                       + cache    60.0   16.7   17.6   20.8     1    31%    598
Slaughterhouse         vanilla    58.8   16.6   34.5   36.5   188    40%    692
                       Cadence    60.0   16.7   18.7   33.3     5    29%    552
                       + cache    60.0   16.7   17.7   31.2     3    30%    553
Abyss of Darkness      vanilla    59.9   16.6   19.6   31.8    10    41%    633
                       Cadence    59.9   16.7   17.5   33.8     2    31%    541
                       + cache    60.0   16.7   17.4   18.0     0    32%    541
Tartarus               vanilla    60.0   16.7   18.3   19.6     0    34%    433
                       Cadence    60.0   16.7   17.9   21.2     3    20%    467
                       + cache    60.0   16.7   17.6   19.0     0    20%    480
Acu                    vanilla    59.9   16.7   18.4   34.1     1    32%    432
                       Cadence    60.0   16.7   17.5   18.0     0    24%    476
                       + cache    60.0   16.7   17.7   23.2     1    25%    494
pc killer              vanilla    55.6   16.6   36.2   37.6   143    47%    820
                       Cadence    60.0   16.7   17.6   27.6     3    36%    722
                       + cache    60.0   16.7   17.9   33.1     3    34%    760
Lag Machine            vanilla    48.3   17.0   50.9   52.6   318    51%    732
                       Cadence    60.0   16.7   17.8   22.2     2    38%    959
                       + cache    59.6   16.7   26.2   34.2    24    36%    966
stress test            vanilla    57.8   16.8   34.1   38.7   244    40%    562
                       Cadence    60.0   16.7   17.4   17.9     0    31%    491
                       + cache    60.0   16.7   17.5   32.0     1    30%    484
Bloodbath              vanilla    60.0   16.7   18.2   18.5     0    29%    383
                       Cadence    59.9   16.7   17.5   33.8     2    19%    454
                       + cache    60.0   16.7   17.5   18.3     0    19%    464
```

Mean over the 15 levels: CPU 39.5% vanilla, 29.0% Cadence, 28.9% with the cache; memory 701 /
593 / 606 MB; frames over 20 ms 1351 / 21 / 37; lowest FPS 48.3 / 59.8 / 59.6.

- Under these conditions Cadence uses about a quarter less CPU than the real game on every level
  and holds 60 FPS where the real game drops (Lag Machine 48.3, pc killer 55.6, stress test 57.8,
  Slaughterhouse, Avernus, Firework, Bloodlust). The earlier table, with the real game as the
  focused app and nothing else running, had the real game at about 22% and Cadence at about 32%:
  why both moved (background app, a busy machine, the parked window) isn't pinned down yet, so
  don't compare numbers across the two tables. Cadence's higher MB here is its audio, now on.
- The draw cache makes no measurable difference to CPU (28.9% vs 29.0%) and costs ~13 MB. Lag
  Machine's 24 slow frames with the cache were noise: two repeats each gave 75/62 slow frames
  without it and 82/86 with it, at 33–34% CPU either way. The cache stays (pixel-identical) but
  is not a speed win on this machine.

## Browser (WebAssembly) vs native, same Mac (2026-11-05)

MacBook Air M2, macOS 27, 60 Hz display, nothing else running. Both at 1280×720-ish pixels
(native 640×360 points, the browser canvas 640×356 points at 2×), High texture quality in both,
noclip, measured the same way: from the level's start, 22 s, the first 30 frames left out.
Native: the release build (`CADENCE_BENCH=22`), frame = interval between presented frames.
Browser: Chrome 157 (WebGPU) with its own profile, a 640-wide app window parked in the corner
and never focused (background throttling off), `?dev=1&noclip&level=<id>`; frame = interval
between `requestAnimationFrame` callbacks (`/tmp`-free script: driven over the DevTools
protocol). "Steady" leaves out stalls over 200 ms — in the browser that is a single ~1.5 s
stall right after Play while the level's High-quality sheets go to the GPU.

```
level         run       FPS      steady   p50 ms   p99 ms  >20ms  stalls
Bloodbath     native    59.952   59.952   16.667   17.292     2   —
              native    60.002   60.002   16.677   17.723     2   —
              browser   55.919   60.000   16.700   17.700     1   1482 ms (loading)
              browser   55.919   59.953   16.700   17.700     2   1467 ms (loading)
Lag Machine   native    57.874   57.874   16.680   33.476   102   —
              native    57.913   57.913   16.685   33.465    93   —
              browser   55.824   60.000   16.700   17.700     1   1517 ms (loading)
              browser   55.870   60.003   16.700   17.700     1   1501 ms (loading)
```

Averages of the two runs, browser − native:

| Level | Native FPS | Browser FPS | Difference | Browser steady | Difference (steady) |
|---|---|---|---|---|---|
| Bloodbath | 59.977 | 55.919 | −4.058 | 59.977 | 0.000 |
| Lag Machine | 57.894 | 55.847 | −2.047 | 60.002 | +2.108 |

- The browser's whole deficit is the loading stall at the start (High textures uploaded when the
  level starts; Medium halves it). Once playing it holds 60.000 on both levels.
- Natively Lag Machine has ~100 frames over 20 ms (frames at 33 ms); in the browser none. The two
  measure different points (presented frames vs animation-frame callbacks), so part of that gap
  can be the measurement, not the build.
- Medium texture quality (the browser page's default) measured 58.607 / 58.564 FPS with a
  ~0.52 s loading stall.
