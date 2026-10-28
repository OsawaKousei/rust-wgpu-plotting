struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) clip: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) clip: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    output.clip = input.clip;
    return output;
}

fn inside_clip(input: VertexOutput) -> bool {
    let pixel = input.position.xy;
    return pixel.x >= input.clip.x && pixel.y >= input.clip.y &&
           pixel.x < input.clip.z && pixel.y < input.clip.w;
}

@fragment
fn fs_solid(input: VertexOutput) -> @location(0) vec4<f32> {
    if !inside_clip(input) { discard; }
    return input.color;
}

@fragment
fn fs_marker(input: VertexOutput) -> @location(0) vec4<f32> {
    if !inside_clip(input) || dot(input.uv, input.uv) > 1.0 { discard; }
    return input.color;
}

@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@fragment
fn fs_image(input: VertexOutput) -> @location(0) vec4<f32> {
    if !inside_clip(input) { discard; }
    // Input textures are uploaded as linear, premultiplied RGBA8. Hardware
    // interpolation therefore preserves color at transparent boundaries.
    return textureSample(source_image, source_sampler, input.uv);
}

@fragment
fn fs_layer(input: VertexOutput) -> @location(0) vec4<f32> {
    if !inside_clip(input) { discard; }
    // Source frames are already linear and premultiplied. Opacity scales both
    // color and alpha before source-over blending.
    return textureSample(source_image, source_sampler, input.uv) * input.color.x;
}

fn presented_linear(input: VertexOutput) -> vec3<f32> {
    let sampled = textureSample(source_image, source_sampler, input.uv);
    return sampled.rgb + vec3<f32>(1.0 - sampled.a);
}

@fragment
fn fs_present_srgb(input: VertexOutput) -> @location(0) vec4<f32> {
    if !inside_clip(input) { discard; }
    return vec4<f32>(presented_linear(input), 1.0);
}

@fragment
fn fs_present_unorm(input: VertexOutput) -> @location(0) vec4<f32> {
    if !inside_clip(input) { discard; }
    let linear = clamp(presented_linear(input), vec3<f32>(0.0), vec3<f32>(1.0));
    let low = 12.92 * linear;
    let high = 1.055 * pow(linear, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    let encoded = select(high, low, linear <= vec3<f32>(0.0031308));
    return vec4<f32>(encoded, 1.0);
}
