// Dissolves the frame from before a change into the one after it, inside a rectangle of the window. The patterns
// are a shader example's three quadrants, each given the whole rectangle and run once, ending exactly on the new
// frame. The example faded to black; here what's revealed is the new frame.

cbuffer Dissolve : register(b0) {
    float2 origin;  // The rectangle's top left, in pixels.
    float2 size;    // Its size, in pixels.
    float progress; // 0 shows the frame before, 1 the frame after.
    uint effect;
    float cell;     // The side of the pattern's square cells, in pixels.
    float padding;
};

Texture2D frame_before : register(t0);
Texture2D frame_after : register(t1);

static const float PI = 3.14159265359;
static const float SQRT2 = 1.41421356237;

// One triangle over the whole target; the scissor keeps it to the rectangle.
float4 vs_main(uint id : SV_VertexID) : SV_POSITION {
    float2 p = float2((id << 1) & 2, id & 2);
    return float4(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
}

// The example's sine wave, as 0 to 1 over half its period.
float ease(float t) {
    return 0.5 - 0.5 * cos(PI * t);
}

// Each of these says how much of the new frame shows: `uv` is across the rectangle (y down), `st` counts cells (y up).

// Diamonds of the old frame shrink away, in a wave from the bottom left.
float diamonds(float2 uv, float2 st) {
    float2 f = frac(st) - 0.5;
    f = abs(float2(f.x - f.y, f.x + f.y) * SQRT2);
    // The part of the example's wave in which anything moves: a whole rectangle's worth, so none is spent waiting.
    float wave = uv.x * 0.5 - lerp(-0.354, 0.708, ease(progress)) * SQRT2;
    float t = 1.0 - smoothstep(wave, wave + 0.5, uv.y * 0.5);
    // About a pixel of softening, and a diamond that starts that much too big, so its corners overlap and no grid
    // of the new frame shows until the wave arrives.
    float soft = 2.83 / cell;
    return 1.0 - saturate((t * (SQRT2 + soft - f.x) - f.y) / soft);
}

// Each cell's gradient runs dark as a soft edge sweeps from the right.
float blinds(float2 uv, float2 st) {
    float t = lerp(0.8, -0.8, ease(progress));
    float n = smoothstep(0.2, 0.8, 1.0 - uv.x + t) * 2.0 - 1.0;
    return 1.0 - saturate(frac(st).x + n);
}

// Every other cell opens from its top, and the rest follow a third of the way behind.
float checkerboard(float2 st) {
    float2 c = floor(st);
    float t = progress * 1.5 - (fmod(c.x + c.y, 2.0) >= 1.0 ? 0.0 : 0.5);
    return saturate(frac(st).y + sin(PI * (saturate(t) - 0.5)));
}

float4 ps_main(float4 pos : SV_POSITION) : SV_TARGET {
    int3 pixel = int3(pos.xy, 0);
    float3 before = frame_before.Load(pixel).rgb;
    float3 after = frame_after.Load(pixel).rgb;
    // What the change didn't touch stays exactly as it was.
    if (all(before == after)) {
        return float4(after, 1.0);
    }
    float2 inside = pos.xy - origin;
    float2 uv = inside / size;
    float2 st = float2(inside.x, size.y - inside.y) / cell;
    float amount;
    if (effect == 1) {
        amount = diamonds(uv, st);
    } else if (effect == 2) {
        amount = blinds(uv, st);
    } else {
        amount = checkerboard(st);
    }
    // Mixed in linear light, as in the example.
    float3 mixed = lerp(pow(before, 2.2), pow(after, 2.2), amount);
    return float4(pow(mixed, 1.0 / 2.2), 1.0);
}
