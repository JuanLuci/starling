// Starling Visuals: the options its screen lists.

#define SUN_STRENGTH 2.4 // [1.5 2.0 2.4 2.8 3.2 3.6]
#define SKY_BRIGHTNESS 1.0 // [0.6 0.8 1.0 1.2 1.4]
#define BLOCKLIGHT_STRENGTH 1.0 // [0.5 0.75 1.0 1.25 1.5]
#define EXPOSURE 0.8 // [0.5 0.6 0.7 0.8 0.9 1.0 1.2 1.4]
#define SATURATION 1.05 // [0.8 0.9 1.0 1.05 1.1 1.2]

// Shadows: the map's size and reach, and how much of it goes near the player.
#define SHADOWS
const int shadowMapResolution = 1024; // [512 1024 2048]
const float shadowDistance = 64.0; // [32.0 48.0 64.0 96.0 128.0]
const float sunPathRotation = -25.0; // [-40.0 -30.0 -25.0 -15.0 0.0 15.0 25.0]
const bool shadowHardwareFiltering = true;
#define SHADOW_DISTORTION 0.9 // [0.0 0.7 0.85 0.9 0.93]
#define SHADOW_FILTER 4 // [1 4]
#define SHADOW_SOFTNESS 1.2 // [0.5 0.8 1.2 1.6 2.4]
