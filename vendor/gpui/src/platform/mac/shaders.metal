#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

float4 hsla_to_rgba(Hsla hsla);
float3 srgb_to_linear(float3 color);
float3 linear_to_srgb(float3 color);
float4 srgb_to_oklab(float4 color);
float4 oklab_to_srgb(float4 color);
float4 to_device_position(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          constant Size_DevicePixels *viewport_size);
float4 to_device_position_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          TransformationMatrix transformation,
                          constant Size_DevicePixels *input_viewport_size);

float2 to_tile_position(float2 unit_vertex, AtlasTile tile,
                        constant Size_DevicePixels *atlas_size);
float4 distance_from_clip_rect(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds);
float4 distance_from_clip_rect_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds, TransformationMatrix transformation);
static inline float content_mask_fade(ContentMask_ScaledPixels mask, float2 fragPx) {
  float fade = mask.bottom_fade;
  if (fade <= 0.5) {
    return 1.0;
  }
  float distFromBottom = (mask.bounds.origin.y + mask.bounds.size.height) - fragPx.y;
  return smoothstep(0.0, fade, distFromBottom);
}
float corner_dash_velocity(float dv1, float dv2);
float dash_alpha(float t, float period, float length, float dash_velocity,
                 float antialias_threshold);
float quarter_ellipse_sdf(float2 point, float2 radii);
float pick_corner_radius(float2 center_to_point, Corners_ScaledPixels corner_radii);
float quad_sdf(float2 point, Bounds_ScaledPixels bounds,
               Corners_ScaledPixels corner_radii);
float quad_sdf_impl(float2 center_to_point, float corner_radius);
float gaussian(float x, float sigma);
float2 erf(float2 x);
float blur_along_x(float x, float y, float sigma, float corner,
                   float2 half_size);
float4 over(float4 below, float4 above);
float radians(float degrees);
float4 fill_color(Background background, float2 position, Bounds_ScaledPixels bounds,
  float4 solid_color, float4 color0, float4 color1);

struct GradientColor {
  float4 solid;
  float4 color0;
  float4 color1;
};
GradientColor prepare_fill_color(uint tag, uint color_space, Hsla solid, Hsla color0, Hsla color1);

struct SolidQuadVertexOutput {
  float4 position [[position]];
  half4 color [[flat]];
  float2 center [[flat]];
  float2 half_size [[flat]];
  float4 corner_radii [[flat]];
  float2 mask_params [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct SolidQuadFragmentInput {
  float4 position [[position]];
  half4 color [[flat]];
  float2 center [[flat]];
  float2 half_size [[flat]];
  float4 corner_radii [[flat]];
  float2 mask_params [[flat]];
};

vertex SolidQuadVertexOutput solid_quad_vertex(
    uint unit_vertex_id [[vertex_id]], uint quad_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(QuadInputIndex_Vertices)]],
    constant Quad *quads [[buffer(QuadInputIndex_Quads)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(QuadInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Quad quad = quads[quad_id];
  float4 bw = float4(quad.border_widths.top, quad.border_widths.right,
                     quad.border_widths.bottom, quad.border_widths.left);
  Bounds_ScaledPixels bounds = quad.bounds;
  half4 color;
  float4 corner_radii;
  if (bw.x > 0.0f || bw.y > 0.0f || bw.z > 0.0f || bw.w > 0.0f) {
    color = half4(hsla_to_rgba(quad.border_color));
    corner_radii = float4(0.0f);
    if (bw.w > 0.0f) {
      bounds.size.width = min(bounds.size.width, bw.w);
    } else if (bw.y > 0.0f) {
      float w = min(bounds.size.width, bw.y);
      bounds.origin.x += bounds.size.width - w;
      bounds.size.width = w;
    } else if (bw.x > 0.0f) {
      bounds.size.height = min(bounds.size.height, bw.x);
    } else {
      float h = min(bounds.size.height, bw.z);
      bounds.origin.y += bounds.size.height - h;
      bounds.size.height = h;
    }
  } else {
    color = half4(hsla_to_rgba(quad.background.solid));
    corner_radii = float4(
        quad.corner_radii.top_left,
        quad.corner_radii.top_right,
        quad.corner_radii.bottom_right,
        quad.corner_radii.bottom_left);
  }
  if (color.a <= 0.0h || bounds.size.width <= 0.0f || bounds.size.height <= 0.0f) {
    return SolidQuadVertexOutput{
        float4(2.0f, 2.0f, 0.0f, 1.0f),
        half4(0.0h),
        float2(0.0f),
        float2(0.0f),
        float4(0.0f),
        float2(0.0f),
        {-1.0f, -1.0f, -1.0f, -1.0f}};
  }
  float4 device_position =
      to_device_position(unit_vertex, bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, bounds,
                                                 quad.content_mask.bounds);
  float2 half_size = float2(bounds.size.width, bounds.size.height) * 0.5;
  float2 center = float2(bounds.origin.x, bounds.origin.y) + half_size;
  float mask_bottom_y =
      quad.content_mask.bounds.origin.y + quad.content_mask.bounds.size.height;

  return SolidQuadVertexOutput{
      device_position,
      color,
      center,
      half_size,
      corner_radii,
      float2(quad.content_mask.bottom_fade, mask_bottom_y),
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment half4 solid_quad_fragment(SolidQuadFragmentInput input [[stage_in]]) {
  float2 rel = input.position.xy - input.center;
  float corner_radius = (rel.x < 0.0)
      ? ((rel.y < 0.0) ? input.corner_radii.x : input.corner_radii.w)
      : ((rel.y < 0.0) ? input.corner_radii.y : input.corner_radii.z);
  float2 q = abs(rel) - input.half_size + corner_radius;

  half alpha = 1.0h;
  if (q.x > -0.5 || q.y > -0.5) {
    float outer_sdf = min(max(q.x, q.y), 0.0) + length(max(q, float2(0.0))) - corner_radius;
    alpha = half(saturate(0.5 - outer_sdf));
  }

  float fade = input.mask_params.x;
  if (fade > 0.5) {
    alpha *= half(smoothstep(0.0, fade, input.mask_params.y - input.position.y));
  }
  return half4(input.color.rgb, input.color.a * alpha);
}

struct QuadVertexOutput {
  uint quad_id [[flat]];
  float4 position [[position]];
  float4 border_color [[flat]];
  float4 background_solid [[flat]];
  float4 background_color0 [[flat]];
  float4 background_color1 [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct QuadFragmentInput {
  uint quad_id [[flat]];
  float4 position [[position]];
  float4 border_color [[flat]];
  float4 background_solid [[flat]];
  float4 background_color0 [[flat]];
  float4 background_color1 [[flat]];
};

vertex QuadVertexOutput quad_vertex(uint unit_vertex_id [[vertex_id]],
                                    uint quad_id [[instance_id]],
                                    constant float2 *unit_vertices
                                    [[buffer(QuadInputIndex_Vertices)]],
                                    constant Quad *quads
                                    [[buffer(QuadInputIndex_Quads)]],
                                    constant Size_DevicePixels *viewport_size
                                    [[buffer(QuadInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Quad quad = quads[quad_id];
  float4 device_position =
      to_device_position(unit_vertex, quad.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, quad.bounds,
                                                 quad.content_mask.bounds);
  float4 border_color = hsla_to_rgba(quad.border_color);

  GradientColor gradient = prepare_fill_color(
    quad.background.tag,
    quad.background.color_space,
    quad.background.solid,
    quad.background.colors[0].color,
    quad.background.colors[1].color
  );

  return QuadVertexOutput{
      quad_id,
      device_position,
      border_color,
      gradient.solid,
      gradient.color0,
      gradient.color1,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment float4 quad_fragment(QuadFragmentInput input [[stage_in]],
                              constant Quad *quads
                              [[buffer(QuadInputIndex_Quads)]]) {
  Quad quad = quads[input.quad_id];
  float4 background_color = fill_color(quad.background, input.position.xy, quad.bounds,
    input.background_solid, input.background_color0, input.background_color1);

  bool unrounded = quad.corner_radii.top_left == 0.0 &&
    quad.corner_radii.bottom_left == 0.0 &&
    quad.corner_radii.top_right == 0.0 &&
    quad.corner_radii.bottom_right == 0.0;

  // Fast path when the quad is not rounded and doesn't have any border
  if (quad.border_widths.top == 0.0 &&
      quad.border_widths.left == 0.0 &&
      quad.border_widths.right == 0.0 &&
      quad.border_widths.bottom == 0.0 &&
      unrounded) {
    return background_color;
  }

  float2 size = float2(quad.bounds.size.width, quad.bounds.size.height);
  float2 half_size = size / 2.0;
  float2 point = input.position.xy - float2(quad.bounds.origin.x, quad.bounds.origin.y);
  float2 center_to_point = point - half_size;

  // Signed distance field threshold for inclusion of pixels. 0.5 is the
  // minimum distance between the center of the pixel and the edge.
  const float antialias_threshold = 0.5;

  // Radius of the nearest corner
  float corner_radius = pick_corner_radius(center_to_point, quad.corner_radii);

  // Width of the nearest borders
  float2 border = float2(
    center_to_point.x < 0.0 ? quad.border_widths.left : quad.border_widths.right,
    center_to_point.y < 0.0 ? quad.border_widths.top : quad.border_widths.bottom
  );

  // 0-width borders are reduced so that `inner_sdf >= antialias_threshold`.
  // The purpose of this is to not draw antialiasing pixels in this case.
  float2 reduced_border = float2(
    border.x == 0.0 ? -antialias_threshold : border.x,
    border.y == 0.0 ? -antialias_threshold : border.y);

  // Vector from the corner of the quad bounds to the point, after mirroring
  // the point into the bottom right quadrant. Both components are <= 0.
  float2 corner_to_point = fabs(center_to_point) - half_size;

  // Vector from the point to the center of the rounded corner's circle, also
  // mirrored into bottom right quadrant.
  float2 corner_center_to_point = corner_to_point + corner_radius;

  // Whether the nearest point on the border is rounded
  bool is_near_rounded_corner =
    corner_center_to_point.x >= 0.0 &&
    corner_center_to_point.y >= 0.0;

  // Vector from straight border inner corner to point.
  //
  // 0-width borders are turned into width -1 so that inner_sdf is > 1.0 near
  // the border. Without this, antialiasing pixels would be drawn.
  float2 straight_border_inner_corner_to_point = corner_to_point + reduced_border;

  // Whether the point is beyond the inner edge of the straight border
  bool is_beyond_inner_straight_border =
    straight_border_inner_corner_to_point.x > 0.0 ||
    straight_border_inner_corner_to_point.y > 0.0;


  // Whether the point is far enough inside the quad, such that the pixels are
  // not affected by the straight border.
  bool is_within_inner_straight_border =
    straight_border_inner_corner_to_point.x < -antialias_threshold &&
    straight_border_inner_corner_to_point.y < -antialias_threshold;

  // Fast path for points that must be part of the background
  if (is_within_inner_straight_border && !is_near_rounded_corner) {
    return background_color;
  }

  // Signed distance of the point to the outside edge of the quad's border
  float outer_sdf = quad_sdf_impl(corner_center_to_point, corner_radius);

  // Approximate signed distance of the point to the inside edge of the quad's
  // border. It is negative outside this edge (within the border), and
  // positive inside.
  //
  // This is not always an accurate signed distance:
  // * The rounded portions with varying border width use an approximation of
  //   nearest-point-on-ellipse.
  // * When it is quickly known to be outside the edge, -1.0 is used.
  float inner_sdf = 0.0;
  if (corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0) {
    // Fast paths for straight borders
    inner_sdf = -max(straight_border_inner_corner_to_point.x,
                     straight_border_inner_corner_to_point.y);
  } else if (is_beyond_inner_straight_border) {
    // Fast path for points that must be outside the inner edge
    inner_sdf = -1.0;
  } else if (reduced_border.x == reduced_border.y) {
    // Fast path for circular inner edge.
    inner_sdf = -(outer_sdf + reduced_border.x);
  } else {
    float2 ellipse_radii = max(float2(0.0), float2(corner_radius) - reduced_border);
    inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
  }

  // Negative when inside the border
  float border_sdf = max(inner_sdf, outer_sdf);

  float4 color = background_color;
  if (border_sdf < antialias_threshold) {
    float4 border_color = input.border_color;

    // Dashed border logic when border_style == 1
    if (quad.border_style == 1) {
      // Position along the perimeter in "dash space", where each dash
      // period has length 1
      float t = 0.0;

      // Total number of dash periods, so that the dash spacing can be
      // adjusted to evenly divide it
      float max_t = 0.0;

      // Border width is proportional to dash size. This is the behavior
      // used by browsers, but also avoids dashes from different segments
      // overlapping when dash size is smaller than the border width.
      //
      // Dash pattern: (2 * border width) dash, (1 * border width) gap
      const float dash_length_per_width = 2.0;
      const float dash_gap_per_width = 1.0;
      const float dash_period_per_width = dash_length_per_width + dash_gap_per_width;

      // Since the dash size is determined by border width, the density of
      // dashes varies. Multiplying a pixel distance by this returns a
      // position in dash space - it has units (dash period / pixels). So
      // a dash velocity of (1 / 10) is 1 dash every 10 pixels.
      float dash_velocity = 0.0;

      // Dividing this by the border width gives the dash velocity
      const float dv_numerator = 1.0 / dash_period_per_width;

      if (unrounded) {
        // When corners aren't rounded, the dashes are separately laid
        // out on each straight line, rather than around the whole
        // perimeter. This way each line starts and ends with a dash.
        bool is_horizontal = corner_center_to_point.x < corner_center_to_point.y;

        // Choosing the right border width for dashed borders.
        // TODO: A better solution exists taking a look at the whole file.
        // this does not fix single dashed borders at the corners
        float2 dashed_border = float2(
        fmax(quad.border_widths.bottom, quad.border_widths.top),
        fmax(quad.border_widths.right, quad.border_widths.left));

        float border_width = is_horizontal ? dashed_border.x : dashed_border.y;
        dash_velocity = dv_numerator / border_width;
        t = is_horizontal ? point.x : point.y;
        t *= dash_velocity;
        max_t = is_horizontal ? size.x : size.y;
        max_t *= dash_velocity;
      } else {
        // When corners are rounded, the dashes are laid out clockwise
        // around the whole perimeter.

        float r_tr = quad.corner_radii.top_right;
        float r_br = quad.corner_radii.bottom_right;
        float r_bl = quad.corner_radii.bottom_left;
        float r_tl = quad.corner_radii.top_left;

        float w_t = quad.border_widths.top;
        float w_r = quad.border_widths.right;
        float w_b = quad.border_widths.bottom;
        float w_l = quad.border_widths.left;

        // Straight side dash velocities
        float dv_t = w_t <= 0.0 ? 0.0 : dv_numerator / w_t;
        float dv_r = w_r <= 0.0 ? 0.0 : dv_numerator / w_r;
        float dv_b = w_b <= 0.0 ? 0.0 : dv_numerator / w_b;
        float dv_l = w_l <= 0.0 ? 0.0 : dv_numerator / w_l;

        // Straight side lengths in dash space
        float s_t = (size.x - r_tl - r_tr) * dv_t;
        float s_r = (size.y - r_tr - r_br) * dv_r;
        float s_b = (size.x - r_br - r_bl) * dv_b;
        float s_l = (size.y - r_bl - r_tl) * dv_l;

        float corner_dash_velocity_tr = corner_dash_velocity(dv_t, dv_r);
        float corner_dash_velocity_br = corner_dash_velocity(dv_b, dv_r);
        float corner_dash_velocity_bl = corner_dash_velocity(dv_b, dv_l);
        float corner_dash_velocity_tl = corner_dash_velocity(dv_t, dv_l);

        // Corner lengths in dash space
        float c_tr = r_tr * (M_PI_F / 2.0) * corner_dash_velocity_tr;
        float c_br = r_br * (M_PI_F / 2.0) * corner_dash_velocity_br;
        float c_bl = r_bl * (M_PI_F / 2.0) * corner_dash_velocity_bl;
        float c_tl = r_tl * (M_PI_F / 2.0) * corner_dash_velocity_tl;

        // Cumulative dash space upto each segment
        float upto_tr = s_t;
        float upto_r = upto_tr + c_tr;
        float upto_br = upto_r + s_r;
        float upto_b = upto_br + c_br;
        float upto_bl = upto_b + s_b;
        float upto_l = upto_bl + c_bl;
        float upto_tl = upto_l + s_l;
        max_t = upto_tl + c_tl;

        if (is_near_rounded_corner) {
          float radians = atan2(corner_center_to_point.y, corner_center_to_point.x);
          float corner_t = radians * corner_radius;

          if (center_to_point.x >= 0.0) {
            if (center_to_point.y < 0.0) {
              dash_velocity = corner_dash_velocity_tr;
              // Subtracted because radians is pi/2 to 0 when
              // going clockwise around the top right corner,
              // since the y axis has been flipped
              t = upto_r - corner_t * dash_velocity;
            } else {
              dash_velocity = corner_dash_velocity_br;
              // Added because radians is 0 to pi/2 when going
              // clockwise around the bottom-right corner
              t = upto_br + corner_t * dash_velocity;
            }
          } else {
            if (center_to_point.y >= 0.0) {
              dash_velocity = corner_dash_velocity_bl;
              // Subtracted because radians is pi/1 to 0 when
              // going clockwise around the bottom-left corner,
              // since the x axis has been flipped
              t = upto_l - corner_t * dash_velocity;
            } else {
              dash_velocity = corner_dash_velocity_tl;
              // Added because radians is 0 to pi/2 when going
              // clockwise around the top-left corner, since both
              // axis were flipped
              t = upto_tl + corner_t * dash_velocity;
            }
          }
        } else {
          // Straight borders
          bool is_horizontal = corner_center_to_point.x < corner_center_to_point.y;
          if (is_horizontal) {
            if (center_to_point.y < 0.0) {
              dash_velocity = dv_t;
              t = (point.x - r_tl) * dash_velocity;
            } else {
              dash_velocity = dv_b;
              t = upto_bl - (point.x - r_bl) * dash_velocity;
            }
          } else {
            if (center_to_point.x < 0.0) {
              dash_velocity = dv_l;
              t = upto_tl - (point.y - r_tl) * dash_velocity;
            } else {
              dash_velocity = dv_r;
              t = upto_r + (point.y - r_tr) * dash_velocity;
            }
          }
        }
      }

      float dash_length = dash_length_per_width / dash_period_per_width;
      float desired_dash_gap = dash_gap_per_width / dash_period_per_width;

      // Straight borders should start and end with a dash, so max_t is
      // reduced to cause this.
      max_t -= unrounded ? dash_length : 0.0;
      if (max_t >= 1.0) {
        // Adjust dash gap to evenly divide max_t
        float dash_count = floor(max_t);
        float dash_period = max_t / dash_count;
        border_color.a *= dash_alpha(t, dash_period, dash_length, dash_velocity,
                                     antialias_threshold);
      } else if (unrounded) {
        // When there isn't enough space for the full gap between the
        // two start / end dashes of a straight border, reduce gap to
        // make them fit.
        float dash_gap = max_t - dash_length;
        if (dash_gap > 0.0) {
          float dash_period = dash_length + dash_gap;
          border_color.a *= dash_alpha(t, dash_period, dash_length, dash_velocity,
                                       antialias_threshold);
        }
      }
    }

    // Blend the border on top of the background and then linearly interpolate
    // between the two as we slide inside the background.
    float4 blended_border = over(background_color, border_color);
    color = mix(background_color, blended_border,
                saturate(antialias_threshold - inner_sdf));
  }

  float mask_fade = content_mask_fade(quad.content_mask, input.position.xy);
  return color * float4(1.0, 1.0, 1.0, saturate(antialias_threshold - outer_sdf) * mask_fade);
}

// Returns the dash velocity of a corner given the dash velocity of the two
// sides, by returning the slower velocity (larger dashes).
//
// Since 0 is used for dash velocity when the border width is 0 (instead of
// +inf), this returns the other dash velocity in that case.
//
// An alternative to this might be to appropriately interpolate the dash
// velocity around the corner, but that seems overcomplicated.
float corner_dash_velocity(float dv1, float dv2) {
  if (dv1 == 0.0) {
    return dv2;
  } else if (dv2 == 0.0) {
    return dv1;
  } else {
    return min(dv1, dv2);
  }
}

// Returns alpha used to render antialiased dashes.
// `t` is within the dash when `fmod(t, period) < length`.
float dash_alpha(
    float t, float period, float length, float dash_velocity,
    float antialias_threshold) {
  float half_period = period / 2.0;
  float half_length = length / 2.0;
  // Value in [-half_period, half_period]
  // The dash is in [-half_length, half_length]
  float centered = fmod(t + half_period - half_length, period) - half_period;
  // Signed distance for the dash, negative values are inside the dash
  float signed_distance = abs(centered) - half_length;
  // Antialiased alpha based on the signed distance
  return saturate(antialias_threshold - signed_distance / dash_velocity);
}

// This approximates distance to the nearest point to a quarter ellipse in a way
// that is sufficient for anti-aliasing when the ellipse is not very eccentric.
// The components of `point` are expected to be positive.
//
// Negative on the outside and positive on the inside.
float quarter_ellipse_sdf(float2 point, float2 radii) {
  // Scale the space to treat the ellipse like a unit circle
  float2 circle_vec = point / radii;
  float unit_circle_sdf = length(circle_vec) - 1.0;
  // Approximate up-scaling of the length by using the average of the radii.
  //
  // TODO: A better solution would be to use the gradient of the implicit
  // function for an ellipse to approximate a scaling factor.
  return unit_circle_sdf * (radii.x + radii.y) * -0.5;
}

struct ShadowVertexOutput {
  float4 position [[position]];
  half4 color [[flat]];
  float2 center [[flat]];
  float2 half_size [[flat]];
  float4 corner_radii [[flat]];
  float4 blur_params [[flat]];
  float3 mask_params [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct ShadowFragmentInput {
  float4 position [[position]];
  half4 color [[flat]];
  float2 center [[flat]];
  float2 half_size [[flat]];
  float4 corner_radii [[flat]];
  float4 blur_params [[flat]];
  float3 mask_params [[flat]];
};

vertex ShadowVertexOutput shadow_vertex(
    uint unit_vertex_id [[vertex_id]], uint shadow_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(ShadowInputIndex_Vertices)]],
    constant Shadow *shadows [[buffer(ShadowInputIndex_Shadows)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(ShadowInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Shadow shadow = shadows[shadow_id];

  float sigma = shadow.blur_radius;
  float4 rgba_color = hsla_to_rgba(shadow.color);
  float sigma_mult = (rgba_color.a <= 0.12) ? 2.4 : ((rgba_color.a <= 0.25) ? 2.65 : 3.0);
  float margin = sigma_mult * sigma;
  float2 orig = float2(shadow.bounds.origin.x, shadow.bounds.origin.y);
  float2 sz = float2(shadow.bounds.size.width, shadow.bounds.size.height);
  float2 half_size = sz * 0.5;
  float2 center = orig + half_size;

  Bounds_ScaledPixels bounds = shadow.bounds;
  bounds.origin.x -= margin;
  bounds.origin.y -= margin;
  bounds.size.width += 2.0 * margin;
  bounds.size.height += 2.0 * margin;

  float4 device_position =
      to_device_position(unit_vertex, bounds, viewport_size);
  float4 clip_distance =
      distance_from_clip_rect(unit_vertex, bounds, shadow.content_mask.bounds);
  half4 color = half4(rgba_color);

  float safe_sigma = max(sigma, 1e-4);
  float inv_sqrt2_sigma = 0.70710678 / safe_sigma;
  float inv_2sigma2 = 0.5 / (safe_sigma * safe_sigma);
  float inv_sqrt2pi_sigma = 0.39894228 / safe_sigma;
  float4 corner_radii = float4(
      shadow.corner_radii.top_left,
      shadow.corner_radii.top_right,
      shadow.corner_radii.bottom_right,
      shadow.corner_radii.bottom_left);
  float mask_bottom_y =
      shadow.content_mask.bounds.origin.y + shadow.content_mask.bounds.size.height;

  return ShadowVertexOutput{
      device_position,
      color,
      center,
      half_size,
      corner_radii,
      float4(sigma, margin, inv_sqrt2_sigma, inv_2sigma2),
      float3(shadow.content_mask.bottom_fade, mask_bottom_y, inv_sqrt2pi_sigma),
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

static inline half2 erf_fast_vec2_half(half2 x) {
  half2 ax = min(abs(x), half2(4.0h));
  half2 p = fma(ax, fma(ax, fma(ax, fma(ax, 0.078108h, 0.000972h), 0.230389h), 0.278393h), 1.0h);
  half2 inv_p = 1.0h / p;
  half2 inv_p2 = inv_p * inv_p;
  half2 r = 1.0h - (inv_p2 * inv_p2);
  return copysign(r, x);
}

static inline half4 erf_fast_vec4_half(half4 x) {
  half4 ax = min(abs(x), half4(4.0h));
  half4 p = fma(ax, fma(ax, fma(ax, fma(ax, 0.078108h, 0.000972h), 0.230389h), 0.278393h), 1.0h);
  half4 inv_p = 1.0h / p;
  half4 inv_p2 = inv_p * inv_p;
  half4 r = 1.0h - (inv_p2 * inv_p2);
  return copysign(r, x);
}

fragment half4 shadow_fragment(ShadowFragmentInput input [[stage_in]]) {
  float2 half_size = input.half_size;
  float2 point = input.position.xy - input.center;
  float2 abs_p = abs(point);

  float corner_radius = (point.x < 0.0f)
      ? ((point.y < 0.0f) ? input.corner_radii.x : input.corner_radii.w)
      : ((point.y < 0.0f) ? input.corner_radii.y : input.corner_radii.z);

  float sigma = input.blur_params.x;
  half alpha;
  if (sigma <= 0.0f) {
    float r = min(corner_radius, min(half_size.x, half_size.y));
    float2 q = abs_p - half_size + r;
    float distance = min(max(q.x, q.y), 0.0f) + length(max(q, float2(0.0f))) - r;
    alpha = half(saturate(0.5f - distance));
  } else {
    float cutoff = input.blur_params.y;
    float inv_sqrt2_sigma = input.blur_params.z;

    float2 straight_limit = half_size - corner_radius - cutoff;
    float2 inner_limit = half_size - cutoff;

    if (abs_p.x <= inner_limit.x && abs_p.y <= inner_limit.y &&
        (abs_p.x <= straight_limit.x || abs_p.y <= straight_limit.y)) {
      alpha = 1.0h;
    } else if (abs_p.y <= straight_limit.y) {
      half2 e = erf_fast_vec2_half(half2((abs_p.x - half_size.x) * inv_sqrt2_sigma,
                                         (abs_p.x + half_size.x) * inv_sqrt2_sigma));
      alpha = 0.5h * (e.y - e.x);
    } else if (abs_p.x <= straight_limit.x) {
      float start = clamp(-cutoff, abs_p.y - half_size.y, abs_p.y + half_size.y);
      float end = clamp(cutoff, abs_p.y - half_size.y, abs_p.y + half_size.y);
      half2 ey = erf_fast_vec2_half(half2(end * inv_sqrt2_sigma, start * inv_sqrt2_sigma));
      alpha = 0.5h * (ey.x - ey.y);
    } else if (corner_radius * corner_radius * float(input.color.a) <= 0.08f * sigma * sigma) {
      half4 e = erf_fast_vec4_half(half4(
          (abs_p.x - half_size.x) * inv_sqrt2_sigma,
          (abs_p.x + half_size.x) * inv_sqrt2_sigma,
          (abs_p.y - half_size.y) * inv_sqrt2_sigma,
          (abs_p.y + half_size.y) * inv_sqrt2_sigma));
      alpha = 0.25h * (e.y - e.x) * (e.w - e.z);
    } else {
      float2 cq = abs_p - half_size + corner_radius;
      float outer_r = cutoff + corner_radius;
      if (cq.x > 0.0f && cq.y > 0.0f && dot(cq, cq) > outer_r * outer_r) {
        discard_fragment();
      }
      float low = abs_p.y - half_size.y;
      float high = abs_p.y + half_size.y;
      float start = clamp(-cutoff, low, high);
      float end = clamp(cutoff, low, high);
      float span = end - start;
      if (span <= 0.0f) {
        alpha = 0.0h;
      } else {
        float step = span * 0.25f;
        float4 y4 = start + step * float4(0.5f, 1.5f, 2.5f, 3.5f);
        float inv_2sigma2 = input.blur_params.w;
        half4 w4 = half4(fast::exp(-(y4 * y4) * inv_2sigma2));
        float4 delta4 = min(
            float4(half_size.y - corner_radius) - abs(float4(abs_p.y) - y4),
            float4(0.0f));
        float4 curved4 = float4(half_size.x - corner_radius)
            + sqrt(max(float4(0.0f), float4(corner_radius * corner_radius) - delta4 * delta4));
        half4 e_lo = erf_fast_vec4_half(half4((float4(abs_p.x) - curved4) * inv_sqrt2_sigma));
        half4 bx4;
        if (abs_p.x + (half_size.x - corner_radius) > 3.2f * sigma) {
          bx4 = 0.5h * (1.0h - e_lo);
        } else {
          half4 e_hi = erf_fast_vec4_half(half4((float4(abs_p.x) + curved4) * inv_sqrt2_sigma));
          bx4 = 0.5h * (e_hi - e_lo);
        }
        alpha = dot(bx4, w4) * half(input.mask_params.z * step);
      }
    }
  }

  float fade = input.mask_params.x;
  half mask_fade = (fade <= 0.5f)
      ? 1.0h
      : half(smoothstep(0.0f, fade, input.mask_params.y - input.position.y));
  return input.color * half4(1.0h, 1.0h, 1.0h, alpha * mask_fade);
}

struct ShadowSolidVertexOutput {
  float4 position [[position]];
  half4 color [[flat]];
  float2 mask_params [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct ShadowSolidFragmentInput {
  float4 position [[position]];
  half4 color [[flat]];
  float2 mask_params [[flat]];
};

struct ShadowEdgeVertexOutput {
  float4 position [[position]];
  half4 color [[flat]];
  float4 edge_params [[flat]];
  float4 mask_cutoff [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct ShadowEdgeFragmentInput {
  float4 position [[position]];
  half4 color [[flat]];
  float4 edge_params [[flat]];
  float4 mask_cutoff [[flat]];
};

static inline Bounds_ScaledPixels make_sub_bounds(float x0, float y0, float x1, float y1) {
  Bounds_ScaledPixels b;
  b.origin.x = x0;
  b.origin.y = y0;
  b.size.width = max(x1 - x0, 0.0f);
  b.size.height = max(y1 - y0, 0.0f);
  return b;
}

static inline void compute_shadow_9slice(
    Shadow shadow,
    float margin,
    float shadow_alpha,
    thread float &ox0, thread float &oy0, thread float &ox1, thread float &oy1,
    thread float &ix0, thread float &iy0, thread float &ix1, thread float &iy1) {
  float2 orig = float2(shadow.bounds.origin.x, shadow.bounds.origin.y);
  float2 sz = float2(shadow.bounds.size.width, shadow.bounds.size.height);
  float2 half_size = sz * 0.5f;
  float2 center = orig + half_size;
  ox0 = floor(orig.x - margin);
  oy0 = floor(orig.y - margin);
  ox1 = ceil(orig.x + sz.x + margin);
  oy1 = ceil(orig.y + sz.y + margin);
  if (shadow.blur_radius <= 0.0f) {
    ix0 = ox0;
    ix1 = ox0;
    iy0 = oy0;
    iy1 = oy0;
    return;
  }
  float max_r = max(
      max(shadow.corner_radii.top_left, shadow.corner_radii.top_right),
      max(shadow.corner_radii.bottom_right, shadow.corner_radii.bottom_left));
  float eff_r = (max_r * max_r * shadow_alpha <= 0.08f * shadow.blur_radius * shadow.blur_radius)
      ? 0.0f
      : max_r;
  float sx = max(half_size.x - eff_r - margin, 0.0f);
  float sy = max(half_size.y - eff_r - margin, 0.0f);
  ix0 = clamp(ceil(center.x - sx), ox0, ox1);
  ix1 = clamp(floor(center.x + sx), ix0, ox1);
  iy0 = clamp(ceil(center.y - sy), oy0, oy1);
  iy1 = clamp(floor(center.y + sy), iy0, oy1);
}

vertex ShadowSolidVertexOutput shadow_solid_vertex(
    uint unit_vertex_id [[vertex_id]], uint shadow_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(ShadowInputIndex_Vertices)]],
    constant Shadow *shadows [[buffer(ShadowInputIndex_Shadows)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(ShadowInputIndex_ViewportSize)]]) {
  Shadow shadow = shadows[shadow_id];
  float sigma = shadow.blur_radius;
  float4 rgba_color = hsla_to_rgba(shadow.color);
  float sigma_mult = (rgba_color.a <= 0.12f) ? 2.4f : ((rgba_color.a <= 0.25f) ? 2.65f : 3.0f);
  float margin = sigma_mult * sigma;

  float ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1;
  compute_shadow_9slice(shadow, margin, rgba_color.a, ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1);

  float mask_bottom_y =
      shadow.content_mask.bounds.origin.y + shadow.content_mask.bounds.size.height;
  if (ix1 <= ix0 || iy1 <= iy0) {
    return ShadowSolidVertexOutput{
        float4(2.0f, 2.0f, 0.0f, 1.0f),
        half4(rgba_color),
        float2(shadow.content_mask.bottom_fade, mask_bottom_y),
        {-1.0f, -1.0f, -1.0f, -1.0f}};
  }
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Bounds_ScaledPixels bounds = make_sub_bounds(ix0, iy0, ix1, iy1);
  float4 device_position = to_device_position(unit_vertex, bounds, viewport_size);
  float4 clip_distance =
      distance_from_clip_rect(unit_vertex, bounds, shadow.content_mask.bounds);
  return ShadowSolidVertexOutput{
      device_position,
      half4(rgba_color),
      float2(shadow.content_mask.bottom_fade, mask_bottom_y),
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment half4 shadow_solid_fragment(ShadowSolidFragmentInput input [[stage_in]]) {
  float fade = input.mask_params.x;
  half mask_fade = (fade <= 0.5f)
      ? 1.0h
      : half(smoothstep(0.0f, fade, input.mask_params.y - input.position.y));
  return input.color * half4(1.0h, 1.0h, 1.0h, mask_fade);
}

vertex ShadowEdgeVertexOutput shadow_edge_vertex(
    uint vertex_id [[vertex_id]], uint shadow_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(ShadowInputIndex_Vertices)]],
    constant Shadow *shadows [[buffer(ShadowInputIndex_Shadows)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(ShadowInputIndex_ViewportSize)]]) {
  Shadow shadow = shadows[shadow_id];
  float sigma = shadow.blur_radius;
  float4 rgba_color = hsla_to_rgba(shadow.color);
  float sigma_mult = (rgba_color.a <= 0.12f) ? 2.4f : ((rgba_color.a <= 0.25f) ? 2.65f : 3.0f);
  float margin = sigma_mult * sigma;

  float ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1;
  compute_shadow_9slice(shadow, margin, rgba_color.a, ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1);

  uint sub = vertex_id / 6u;
  float rx0, ry0, rx1, ry1;
  float axis_flag;
  if (sub == 0u) {
    rx0 = ix0; ry0 = oy0; rx1 = ix1; ry1 = iy0; axis_flag = 1.0f;
  } else if (sub == 1u) {
    rx0 = ix0; ry0 = iy1; rx1 = ix1; ry1 = oy1; axis_flag = 1.0f;
  } else if (sub == 2u) {
    rx0 = ox0; ry0 = iy0; rx1 = ix0; ry1 = iy1; axis_flag = 0.0f;
  } else {
    rx0 = ix1; ry0 = iy0; rx1 = ox1; ry1 = iy1; axis_flag = 0.0f;
  }

  float mask_bottom_y =
      shadow.content_mask.bounds.origin.y + shadow.content_mask.bounds.size.height;
  if (sigma <= 0.0f || rx1 <= rx0 || ry1 <= ry0) {
    return ShadowEdgeVertexOutput{
        float4(2.0f, 2.0f, 0.0f, 1.0f),
        half4(rgba_color),
        float4(0.0f),
        float4(0.0f),
        {-1.0f, -1.0f, -1.0f, -1.0f}};
  }

  float2 orig = float2(shadow.bounds.origin.x, shadow.bounds.origin.y);
  float2 sz = float2(shadow.bounds.size.width, shadow.bounds.size.height);
  float2 half_size = sz * 0.5f;
  float2 center = orig + half_size;
  float inv_sqrt2_sigma = 0.70710678f / max(sigma, 1e-4f);
  float c_axis = (axis_flag > 0.5f) ? center.y : center.x;
  float h_axis = (axis_flag > 0.5f) ? half_size.y : half_size.x;

  float2 unit_vertex = unit_vertices[vertex_id % 6u];
  Bounds_ScaledPixels bounds = make_sub_bounds(rx0, ry0, rx1, ry1);
  float4 device_position = to_device_position(unit_vertex, bounds, viewport_size);
  float4 clip_distance =
      distance_from_clip_rect(unit_vertex, bounds, shadow.content_mask.bounds);
  return ShadowEdgeVertexOutput{
      device_position,
      half4(rgba_color),
      float4(c_axis, h_axis, inv_sqrt2_sigma, axis_flag),
      float4(shadow.content_mask.bottom_fade, mask_bottom_y, margin, 0.0f),
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment half4 shadow_edge_fragment(ShadowEdgeFragmentInput input [[stage_in]]) {
  float coord = (input.edge_params.w > 0.5f) ? input.position.y : input.position.x;
  float abs_p = abs(coord - input.edge_params.x);
  float h = input.edge_params.y;
  float inv_s = input.edge_params.z;
  half2 e = erf_fast_vec2_half(half2((abs_p - h) * inv_s, (abs_p + h) * inv_s));
  half alpha = 0.5h * (e.y - e.x);
  float fade = input.mask_cutoff.x;
  half mask_fade = (fade <= 0.5f)
      ? 1.0h
      : half(smoothstep(0.0f, fade, input.mask_cutoff.y - input.position.y));
  return input.color * half4(1.0h, 1.0h, 1.0h, alpha * mask_fade);
}

vertex ShadowVertexOutput shadow_corner_vertex(
    uint vertex_id [[vertex_id]], uint shadow_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(ShadowInputIndex_Vertices)]],
    constant Shadow *shadows [[buffer(ShadowInputIndex_Shadows)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(ShadowInputIndex_ViewportSize)]]) {
  Shadow shadow = shadows[shadow_id];
  float sigma = shadow.blur_radius;
  float4 rgba_color = hsla_to_rgba(shadow.color);
  float sigma_mult = (rgba_color.a <= 0.12f) ? 2.4f : ((rgba_color.a <= 0.25f) ? 2.65f : 3.0f);
  float margin = sigma_mult * sigma;

  float ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1;
  compute_shadow_9slice(shadow, margin, rgba_color.a, ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1);

  uint sub = vertex_id / 6u;
  float rx0, ry0, rx1, ry1;
  if (sub == 0u) {
    rx0 = ox0; ry0 = oy0; rx1 = ix0; ry1 = iy0;
  } else if (sub == 1u) {
    rx0 = ix1; ry0 = oy0; rx1 = ox1; ry1 = iy0;
  } else if (sub == 2u) {
    rx0 = ox0; ry0 = iy1; rx1 = ix0; ry1 = oy1;
  } else {
    rx0 = ix1; ry0 = iy1; rx1 = ox1; ry1 = oy1;
  }

  float2 orig = float2(shadow.bounds.origin.x, shadow.bounds.origin.y);
  float2 sz = float2(shadow.bounds.size.width, shadow.bounds.size.height);
  float2 half_size = sz * 0.5f;
  float2 center = orig + half_size;
  float mask_bottom_y =
      shadow.content_mask.bounds.origin.y + shadow.content_mask.bounds.size.height;
  float safe_sigma = max(sigma, 1e-4f);
  float inv_sqrt2_sigma = 0.70710678f / safe_sigma;
  float inv_2sigma2 = 0.5f / (safe_sigma * safe_sigma);
  float inv_sqrt2pi_sigma = 0.39894228f / safe_sigma;
  float4 corner_radii = float4(
      shadow.corner_radii.top_left,
      shadow.corner_radii.top_right,
      shadow.corner_radii.bottom_right,
      shadow.corner_radii.bottom_left);

  if (rx1 <= rx0 || ry1 <= ry0) {
    return ShadowVertexOutput{
        float4(2.0f, 2.0f, 0.0f, 1.0f),
        half4(rgba_color),
        center,
        half_size,
        corner_radii,
        float4(sigma, margin, inv_sqrt2_sigma, inv_2sigma2),
        float3(shadow.content_mask.bottom_fade, mask_bottom_y, inv_sqrt2pi_sigma),
        {-1.0f, -1.0f, -1.0f, -1.0f}};
  }

  float2 unit_vertex = unit_vertices[vertex_id % 6u];
  Bounds_ScaledPixels bounds = make_sub_bounds(rx0, ry0, rx1, ry1);
  float4 device_position = to_device_position(unit_vertex, bounds, viewport_size);
  float4 clip_distance =
      distance_from_clip_rect(unit_vertex, bounds, shadow.content_mask.bounds);
  return ShadowVertexOutput{
      device_position,
      half4(rgba_color),
      center,
      half_size,
      corner_radii,
      float4(sigma, margin, inv_sqrt2_sigma, inv_2sigma2),
      float3(shadow.content_mask.bottom_fade, mask_bottom_y, inv_sqrt2pi_sigma),
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

struct UnderlineVertexOutput {
  float4 position [[position]];
  float4 color [[flat]];
  uint underline_id [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct UnderlineFragmentInput {
  float4 position [[position]];
  float4 color [[flat]];
  uint underline_id [[flat]];
};

vertex UnderlineVertexOutput underline_vertex(
    uint unit_vertex_id [[vertex_id]], uint underline_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(UnderlineInputIndex_Vertices)]],
    constant Underline *underlines [[buffer(UnderlineInputIndex_Underlines)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(ShadowInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Underline underline = underlines[underline_id];
  float4 device_position =
      to_device_position(unit_vertex, underline.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, underline.bounds,
                                                 underline.content_mask.bounds);
  float4 color = hsla_to_rgba(underline.color);
  return UnderlineVertexOutput{
      device_position,
      color,
      underline_id,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment float4 underline_fragment(UnderlineFragmentInput input [[stage_in]],
                                   constant Underline *underlines
                                   [[buffer(UnderlineInputIndex_Underlines)]]) {
  const float WAVE_FREQUENCY = 2.0;
  const float WAVE_HEIGHT_RATIO = 0.8;

  Underline underline = underlines[input.underline_id];
  float mask_fade = content_mask_fade(underline.content_mask, input.position.xy);
  if (underline.wavy) {
    float half_thickness = underline.thickness * 0.5;
    float2 origin =
        float2(underline.bounds.origin.x, underline.bounds.origin.y);

    float2 st = ((input.position.xy - origin) / underline.bounds.size.height) -
                float2(0., 0.5);
    float frequency = (M_PI_F * WAVE_FREQUENCY * underline.thickness) / underline.bounds.size.height;
    float amplitude = (underline.thickness * WAVE_HEIGHT_RATIO) / underline.bounds.size.height;

    float sine = sin(st.x * frequency) * amplitude;
    float dSine = cos(st.x * frequency) * amplitude * frequency;
    float distance = (st.y - sine) / sqrt(1. + dSine * dSine);
    float distance_in_pixels = distance * underline.bounds.size.height;
    float distance_from_top_border = distance_in_pixels - half_thickness;
    float distance_from_bottom_border = distance_in_pixels + half_thickness;
    float alpha = saturate(
        0.5 - max(-distance_from_bottom_border, distance_from_top_border));
    return input.color * float4(1., 1., 1., alpha * mask_fade);
  } else {
    return input.color * float4(1., 1., 1., mask_fade);
  }
}

struct MonochromeSpriteVertexOutput {
  float4 position [[position]];
  float2 tile_position;
  float4 color [[flat]];
  float4 clip_distance;
  uint sprite_id [[flat]];
};

struct MonochromeSpriteFragmentInput {
  float4 position [[position]];
  float2 tile_position;
  float4 color [[flat]];
  float4 clip_distance;
  uint sprite_id [[flat]];
};

vertex MonochromeSpriteVertexOutput monochrome_sprite_vertex(
    uint unit_vertex_id [[vertex_id]], uint sprite_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(SpriteInputIndex_Vertices)]],
    constant MonochromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(SpriteInputIndex_ViewportSize)]],
    constant Size_DevicePixels *atlas_size
    [[buffer(SpriteInputIndex_AtlasTextureSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  MonochromeSprite sprite = sprites[sprite_id];
  float4 device_position =
      to_device_position_transformed(unit_vertex, sprite.bounds, sprite.transformation, viewport_size);
  float4 clip_distance = distance_from_clip_rect_transformed(unit_vertex, sprite.bounds,
                                                 sprite.content_mask.bounds, sprite.transformation);
  float2 tile_position = to_tile_position(unit_vertex, sprite.tile, atlas_size);
  float4 color = hsla_to_rgba(sprite.color);
  return MonochromeSpriteVertexOutput{
      device_position,
      tile_position,
      color,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w},
      sprite_id};
}

fragment float4 monochrome_sprite_fragment(
    MonochromeSpriteFragmentInput input [[stage_in]],
    constant MonochromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    texture2d<float> atlas_texture [[texture(SpriteInputIndex_AtlasTexture)]]) {
  if (any(input.clip_distance < float4(0.0))) {
    return float4(0.0);
  }

  MonochromeSprite sprite = sprites[input.sprite_id];
  float mask_fade = content_mask_fade(sprite.content_mask, input.position.xy);
  constexpr sampler atlas_texture_sampler(mag_filter::linear,
                                          min_filter::linear);
  float4 sample =
      atlas_texture.sample(atlas_texture_sampler, input.tile_position);
  float4 color = input.color;
  color.a *= sample.a * mask_fade;
  return color;
}

struct PolychromeSpriteVertexOutput {
  float4 position [[position]];
  float2 tile_position;
  uint sprite_id [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct PolychromeSpriteFragmentInput {
  float4 position [[position]];
  float2 tile_position;
  uint sprite_id [[flat]];
};

vertex PolychromeSpriteVertexOutput polychrome_sprite_vertex(
    uint unit_vertex_id [[vertex_id]], uint sprite_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(SpriteInputIndex_Vertices)]],
    constant PolychromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(SpriteInputIndex_ViewportSize)]],
    constant Size_DevicePixels *atlas_size
    [[buffer(SpriteInputIndex_AtlasTextureSize)]]) {

  float2 unit_vertex = unit_vertices[unit_vertex_id];
  PolychromeSprite sprite = sprites[sprite_id];
  float4 device_position =
      to_device_position(unit_vertex, sprite.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, sprite.bounds,
                                                 sprite.content_mask.bounds);
  float2 tile_position = to_tile_position(unit_vertex, sprite.tile, atlas_size);
  return PolychromeSpriteVertexOutput{
      device_position,
      tile_position,
      sprite_id,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment float4 polychrome_sprite_fragment(
    PolychromeSpriteFragmentInput input [[stage_in]],
    constant PolychromeSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
    texture2d<float> atlas_texture [[texture(SpriteInputIndex_AtlasTexture)]]) {
  PolychromeSprite sprite = sprites[input.sprite_id];
  constexpr sampler atlas_texture_sampler(mag_filter::linear,
                                          min_filter::linear);
  float4 sample =
      atlas_texture.sample(atlas_texture_sampler, input.tile_position);
  float distance =
      quad_sdf(input.position.xy, sprite.bounds, sprite.corner_radii);

  float4 color = sample;
  if (sprite.grayscale) {
    float grayscale = 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
    color.r = grayscale;
    color.g = grayscale;
    color.b = grayscale;
  }
  float mask_fade = content_mask_fade(sprite.content_mask, input.position.xy);
  color.a *= sprite.opacity * saturate(0.5 - distance) * mask_fade;
  return color;
}

struct PathRasterizationVertexOutput {
  float4 position [[position]];
  float2 st_position;
  uint vertex_id [[flat]];
  float clip_rect_distance [[clip_distance]][4];
};

struct PathRasterizationFragmentInput {
  float4 position [[position]];
  float2 st_position;
  uint vertex_id [[flat]];
};

vertex PathRasterizationVertexOutput path_rasterization_vertex(
  uint vertex_id [[vertex_id]],
  constant PathRasterizationVertex *vertices [[buffer(PathRasterizationInputIndex_Vertices)]],
  constant Size_DevicePixels *atlas_size [[buffer(PathRasterizationInputIndex_ViewportSize)]]
) {
  PathRasterizationVertex v = vertices[vertex_id];
  float2 vertex_position = float2(v.xy_position.x, v.xy_position.y);
  float4 position = float4(
    vertex_position * float2(2. / atlas_size->width, -2. / atlas_size->height) + float2(-1., 1.),
    0.,
    1.
  );
  return PathRasterizationVertexOutput{
      position,
      float2(v.st_position.x, v.st_position.y),
      vertex_id,
      {
        v.xy_position.x - v.bounds.origin.x,
        v.bounds.origin.x + v.bounds.size.width - v.xy_position.x,
        v.xy_position.y - v.bounds.origin.y,
        v.bounds.origin.y + v.bounds.size.height - v.xy_position.y
      }
  };
}

fragment float4 path_rasterization_fragment(
  PathRasterizationFragmentInput input [[stage_in]],
  constant PathRasterizationVertex *vertices [[buffer(PathRasterizationInputIndex_Vertices)]]
) {
  float2 dx = dfdx(input.st_position);
  float2 dy = dfdy(input.st_position);

  PathRasterizationVertex v = vertices[input.vertex_id];
  Background background = v.color;
  Bounds_ScaledPixels path_bounds = v.bounds;
  float alpha;
  if (length(float2(dx.x, dy.x)) < 0.001) {
    alpha = 1.0;
  } else {
    float2 gradient = float2(
      (2. * input.st_position.x) * dx.x - dx.y,
      (2. * input.st_position.x) * dy.x - dy.y
    );
    float f = (input.st_position.x * input.st_position.x) - input.st_position.y;
    float distance = f / length(gradient);
    alpha = saturate(0.5 - distance);
  }

  GradientColor gradient_color = prepare_fill_color(
    background.tag,
    background.color_space,
    background.solid,
    background.colors[0].color,
    background.colors[1].color
  );

  float4 color = fill_color(
    background,
    input.position.xy,
    path_bounds,
    gradient_color.solid,
    gradient_color.color0,
    gradient_color.color1
  );
  return float4(color.rgb * color.a * alpha, alpha * color.a);
}

struct PathSpriteVertexOutput {
  float4 position [[position]];
  float2 texture_coords;
};

vertex PathSpriteVertexOutput path_sprite_vertex(
  uint unit_vertex_id [[vertex_id]],
  uint sprite_id [[instance_id]],
  constant float2 *unit_vertices [[buffer(SpriteInputIndex_Vertices)]],
  constant PathSprite *sprites [[buffer(SpriteInputIndex_Sprites)]],
  constant Size_DevicePixels *viewport_size [[buffer(SpriteInputIndex_ViewportSize)]]
) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  PathSprite sprite = sprites[sprite_id];
  // Don't apply content mask because it was already accounted for when
  // rasterizing the path.
  float4 device_position =
      to_device_position(unit_vertex, sprite.bounds, viewport_size);

  float2 screen_position = float2(sprite.bounds.origin.x, sprite.bounds.origin.y) + unit_vertex * float2(sprite.bounds.size.width, sprite.bounds.size.height);
  float2 texture_coords = screen_position / float2(viewport_size->width, viewport_size->height);

  return PathSpriteVertexOutput{
    device_position,
    texture_coords
  };
}

fragment float4 path_sprite_fragment(
  PathSpriteVertexOutput input [[stage_in]],
  texture2d<float> intermediate_texture [[texture(SpriteInputIndex_AtlasTexture)]]
) {
  constexpr sampler intermediate_texture_sampler(mag_filter::linear, min_filter::linear);
  return intermediate_texture.sample(intermediate_texture_sampler, input.texture_coords);
}

struct SurfaceVertexOutput {
  float4 position [[position]];
  float2 texture_position;
  float clip_distance [[clip_distance]][4];
};

struct SurfaceFragmentInput {
  float4 position [[position]];
  float2 texture_position;
};

vertex SurfaceVertexOutput surface_vertex(
    uint unit_vertex_id [[vertex_id]], uint surface_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(SurfaceInputIndex_Vertices)]],
    constant SurfaceBounds *surfaces [[buffer(SurfaceInputIndex_Surfaces)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(SurfaceInputIndex_ViewportSize)]],
    constant Size_DevicePixels *texture_size
    [[buffer(SurfaceInputIndex_TextureSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  SurfaceBounds surface = surfaces[surface_id];
  float4 device_position =
      to_device_position(unit_vertex, surface.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, surface.bounds,
                                                 surface.content_mask.bounds);
  // We are going to copy the whole texture, so the texture position corresponds
  // to the current vertex of the unit triangle.
  float2 texture_position = unit_vertex;
  return SurfaceVertexOutput{
      device_position,
      texture_position,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment float4 surface_fragment(SurfaceFragmentInput input [[stage_in]],
                                 texture2d<float> y_texture
                                 [[texture(SurfaceInputIndex_YTexture)]],
                                 texture2d<float> cb_cr_texture
                                 [[texture(SurfaceInputIndex_CbCrTexture)]]) {
  constexpr sampler texture_sampler(mag_filter::linear, min_filter::linear);
  const float4x4 ycbcrToRGBTransform =
      float4x4(float4(+1.0000f, +1.0000f, +1.0000f, +0.0000f),
               float4(+0.0000f, -0.3441f, +1.7720f, +0.0000f),
               float4(+1.4020f, -0.7141f, +0.0000f, +0.0000f),
               float4(-0.7010f, +0.5291f, -0.8860f, +1.0000f));
  float4 ycbcr = float4(
      y_texture.sample(texture_sampler, input.texture_position).r,
      cb_cr_texture.sample(texture_sampler, input.texture_position).rg, 1.0);

  return ycbcrToRGBTransform * ycbcr;
}

float4 hsla_to_rgba(Hsla hsla) {
  float h = hsla.h * 6.0; // Now, it's an angle but scaled in [0, 6) range
  float s = hsla.s;
  float l = hsla.l;
  float a = hsla.a;

  float c = (1.0 - fabs(2.0 * l - 1.0)) * s;
  float x = c * (1.0 - fabs(fmod(h, 2.0) - 1.0));
  float m = l - c / 2.0;

  float r = 0.0;
  float g = 0.0;
  float b = 0.0;

  if (h >= 0.0 && h < 1.0) {
    r = c;
    g = x;
    b = 0.0;
  } else if (h >= 1.0 && h < 2.0) {
    r = x;
    g = c;
    b = 0.0;
  } else if (h >= 2.0 && h < 3.0) {
    r = 0.0;
    g = c;
    b = x;
  } else if (h >= 3.0 && h < 4.0) {
    r = 0.0;
    g = x;
    b = c;
  } else if (h >= 4.0 && h < 5.0) {
    r = x;
    g = 0.0;
    b = c;
  } else {
    r = c;
    g = 0.0;
    b = x;
  }

  float4 rgba;
  rgba.x = (r + m);
  rgba.y = (g + m);
  rgba.z = (b + m);
  rgba.w = a;
  return rgba;
}

float3 srgb_to_linear(float3 color) {
  return pow(color, float3(2.2));
}

float3 linear_to_srgb(float3 color) {
  return pow(color, float3(1.0 / 2.2));
}

// Converts a sRGB color to the Oklab color space.
// Reference: https://bottosson.github.io/posts/oklab/#converting-from-linear-srgb-to-oklab
float4 srgb_to_oklab(float4 color) {
  // Convert non-linear sRGB to linear sRGB
  color = float4(srgb_to_linear(color.rgb), color.a);

  float l = 0.4122214708 * color.r + 0.5363325363 * color.g + 0.0514459929 * color.b;
  float m = 0.2119034982 * color.r + 0.6806995451 * color.g + 0.1073969566 * color.b;
  float s = 0.0883024619 * color.r + 0.2817188376 * color.g + 0.6299787005 * color.b;

  float l_ = pow(l, 1.0/3.0);
  float m_ = pow(m, 1.0/3.0);
  float s_ = pow(s, 1.0/3.0);

  return float4(
   	0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
   	1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
   	0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
   	color.a
  );
}

// Converts an Oklab color to the sRGB color space.
float4 oklab_to_srgb(float4 color) {
  float l_ = color.r + 0.3963377774 * color.g + 0.2158037573 * color.b;
  float m_ = color.r - 0.1055613458 * color.g - 0.0638541728 * color.b;
  float s_ = color.r - 0.0894841775 * color.g - 1.2914855480 * color.b;

  float l = l_ * l_ * l_;
  float m = m_ * m_ * m_;
  float s = s_ * s_ * s_;

  float3 linear_rgb = float3(
   	4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
   	-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
   	-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s
  );

  // Convert linear sRGB to non-linear sRGB
  return float4(linear_to_srgb(linear_rgb), color.a);
}

float4 to_device_position(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          constant Size_DevicePixels *input_viewport_size) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);
  float2 viewport_size = float2((float)input_viewport_size->width,
                                (float)input_viewport_size->height);
  float2 device_position =
      position / viewport_size * float2(2., -2.) + float2(-1., 1.);
  return float4(device_position, 0., 1.);
}

float4 to_device_position_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                          TransformationMatrix transformation,
                          constant Size_DevicePixels *input_viewport_size) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);

  // Apply the transformation matrix to the position via matrix multiplication.
  float2 transformed_position = float2(0, 0);
  transformed_position[0] = position[0] * transformation.rotation_scale[0][0] + position[1] * transformation.rotation_scale[0][1];
  transformed_position[1] = position[0] * transformation.rotation_scale[1][0] + position[1] * transformation.rotation_scale[1][1];

  // Add in the translation component of the transformation matrix.
  transformed_position[0] += transformation.translation[0];
  transformed_position[1] += transformation.translation[1];

  float2 viewport_size = float2((float)input_viewport_size->width,
                                (float)input_viewport_size->height);
  float2 device_position =
      transformed_position / viewport_size * float2(2., -2.) + float2(-1., 1.);
  return float4(device_position, 0., 1.);
}


float2 to_tile_position(float2 unit_vertex, AtlasTile tile,
                        constant Size_DevicePixels *atlas_size) {
  float2 tile_origin = float2(tile.bounds.origin.x, tile.bounds.origin.y);
  float2 tile_size = float2(tile.bounds.size.width, tile.bounds.size.height);
  return (tile_origin + unit_vertex * tile_size) /
         float2((float)atlas_size->width, (float)atlas_size->height);
}

// Selects corner radius based on quadrant.
float pick_corner_radius(float2 center_to_point, Corners_ScaledPixels corner_radii) {
  if (center_to_point.x < 0.) {
    if (center_to_point.y < 0.) {
      return corner_radii.top_left;
    } else {
      return corner_radii.bottom_left;
    }
  } else {
    if (center_to_point.y < 0.) {
      return corner_radii.top_right;
    } else {
      return corner_radii.bottom_right;
    }
  }
}

// Signed distance of the point to the quad's border - positive outside the
// border, and negative inside.
float quad_sdf(float2 point, Bounds_ScaledPixels bounds,
               Corners_ScaledPixels corner_radii) {
    float2 half_size = float2(bounds.size.width, bounds.size.height) / 2.0;
    float2 center = float2(bounds.origin.x, bounds.origin.y) + half_size;
    float2 center_to_point = point - center;
    float corner_radius = pick_corner_radius(center_to_point, corner_radii);
    float2 corner_to_point = fabs(center_to_point) - half_size;
    float2 corner_center_to_point = corner_to_point + corner_radius;
    return quad_sdf_impl(corner_center_to_point, corner_radius);
}

// Implementation of quad signed distance field
float quad_sdf_impl(float2 corner_center_to_point, float corner_radius) {
    if (corner_radius == 0.0) {
        // Fast path for unrounded corners
        return max(corner_center_to_point.x, corner_center_to_point.y);
    } else {
        // Signed distance of the point from a quad that is inset by corner_radius
        // It is negative inside this quad, and positive outside
        float signed_distance_to_inset_quad =
            // 0 inside the inset quad, and positive outside
            length(max(float2(0.0), corner_center_to_point)) +
            // 0 outside the inset quad, and negative inside
            min(0.0, max(corner_center_to_point.x, corner_center_to_point.y));

        return signed_distance_to_inset_quad - corner_radius;
    }
}

// A standard gaussian function, used for weighting samples
float gaussian(float x, float sigma) {
  return exp(-(x * x) / (2. * sigma * sigma)) / (sqrt(2. * M_PI_F) * sigma);
}

// This approximates the error function, needed for the gaussian integral
float2 erf(float2 x) {
  float2 s = sign(x);
  float2 a = abs(x);
  float2 r1 = 1. + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
  float2 r2 = r1 * r1;
  return s - s / (r2 * r2);
}

float blur_along_x(float x, float y, float sigma, float corner,
                   float2 half_size) {
  float delta = min(half_size.y - corner - abs(y), 0.);
  float curved =
      half_size.x - corner + sqrt(max(0., corner * corner - delta * delta));
  float2 integral =
      0.5 + 0.5 * erf((x + float2(-curved, curved)) * (sqrt(0.5) / sigma));
  return integral.y - integral.x;
}

float4 distance_from_clip_rect(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);
  return float4(position.x - clip_bounds.origin.x,
                clip_bounds.origin.x + clip_bounds.size.width - position.x,
                position.y - clip_bounds.origin.y,
                clip_bounds.origin.y + clip_bounds.size.height - position.y);
}

float4 distance_from_clip_rect_transformed(float2 unit_vertex, Bounds_ScaledPixels bounds,
                               Bounds_ScaledPixels clip_bounds, TransformationMatrix transformation) {
  float2 position =
      unit_vertex * float2(bounds.size.width, bounds.size.height) +
      float2(bounds.origin.x, bounds.origin.y);
  float2 transformed_position = float2(0, 0);
  transformed_position[0] = position[0] * transformation.rotation_scale[0][0] + position[1] * transformation.rotation_scale[0][1];
  transformed_position[1] = position[0] * transformation.rotation_scale[1][0] + position[1] * transformation.rotation_scale[1][1];
  transformed_position[0] += transformation.translation[0];
  transformed_position[1] += transformation.translation[1];

  return float4(transformed_position.x - clip_bounds.origin.x,
                clip_bounds.origin.x + clip_bounds.size.width - transformed_position.x,
                transformed_position.y - clip_bounds.origin.y,
                clip_bounds.origin.y + clip_bounds.size.height - transformed_position.y);
}

float4 over(float4 below, float4 above) {
  float4 result;
  float alpha = above.a + below.a * (1.0 - above.a);
  result.rgb =
      (above.rgb * above.a + below.rgb * below.a * (1.0 - above.a)) / alpha;
  result.a = alpha;
  return result;
}

GradientColor prepare_fill_color(uint tag, uint color_space, Hsla solid,
                                     Hsla color0, Hsla color1) {
  GradientColor out;
  if (tag == 0 || tag == 2 || tag == 3 || tag == 4) {
    out.solid = hsla_to_rgba(solid);
  } else if (tag == 1) {
    out.color0 = hsla_to_rgba(color0);
    out.color1 = hsla_to_rgba(color1);

    // Prepare color space in vertex for avoid conversion
    // in fragment shader for performance reasons
    if (color_space == 1) {
      // Oklab
      out.color0 = srgb_to_oklab(out.color0);
      out.color1 = srgb_to_oklab(out.color1);
    }
  }

  return out;
}

float2x2 rotate2d(float angle) {
    float s = sin(angle);
    float c = cos(angle);
    return float2x2(c, -s, s, c);
}

float4 fill_color(Background background,
                      float2 position,
                      Bounds_ScaledPixels bounds,
                      float4 solid_color, float4 color0, float4 color1) {
  float4 color;

  switch (background.tag) {
    case 0:
      color = solid_color;
      break;
    case 1: {
      // -90 degrees to match the CSS gradient angle.
      float gradient_angle = background.gradient_angle_or_pattern_height;
      float radians = (fmod(gradient_angle, 360.0) - 90.0) * (M_PI_F / 180.0);
      float2 direction = float2(cos(radians), sin(radians));

      // Expand the short side to be the same as the long side
      if (bounds.size.width > bounds.size.height) {
          direction.y *= bounds.size.height / bounds.size.width;
      } else {
          direction.x *=  bounds.size.width / bounds.size.height;
      }

      // Get the t value for the linear gradient with the color stop percentages.
      float2 half_size = float2(bounds.size.width, bounds.size.height) / 2.;
      float2 center = float2(bounds.origin.x, bounds.origin.y) + half_size;
      float2 center_to_point = position - center;
      float t = dot(center_to_point, direction) / length(direction);
      // Check the direction to determine whether to use x or y
      if (abs(direction.x) > abs(direction.y)) {
          t = (t + half_size.x) / bounds.size.width;
      } else {
          t = (t + half_size.y) / bounds.size.height;
      }

      // Adjust t based on the stop percentages
      t = (t - background.colors[0].percentage)
        / (background.colors[1].percentage
        - background.colors[0].percentage);
      t = clamp(t, 0.0, 1.0);

      switch (background.color_space) {
        case 0:
          color = mix(color0, color1, t);
          break;
        case 1: {
          float4 oklab_color = mix(color0, color1, t);
          color = oklab_to_srgb(oklab_color);
          break;
        }
      }
      break;
    }
    case 2: {
        float gradient_angle_or_pattern_height = background.gradient_angle_or_pattern_height;
        float pattern_width = (gradient_angle_or_pattern_height / 65535.0f) / 255.0f;
        float pattern_interval = fmod(gradient_angle_or_pattern_height, 65535.0f) / 255.0f;
        float pattern_height = pattern_width + pattern_interval;
        float stripe_angle = M_PI_F / 4.0;
        float pattern_period = pattern_height * sin(stripe_angle);
        float2x2 rotation = rotate2d(stripe_angle);
        float2 relative_position = position - float2(bounds.origin.x, bounds.origin.y);
        float2 rotated_point = rotation * relative_position;
        float pattern = fmod(rotated_point.x, pattern_period);
        float distance = min(pattern, pattern_period - pattern) - pattern_period * (pattern_width / pattern_height) /  2.0f;
        color = solid_color;
        color.a *= saturate(0.5 - distance);
        break;
    }
    case 3: {
        color = solid_color;
        break;
    }
    case 4: {
        color = solid_color;
        break;
    }
  }

  return color;
}

static float2 pw_hash22(float2 p) {
  float3 p3 = fract(float3(p.xyx) * float3(0.1031, 0.1030, 0.0973));
  p3 += dot(p3, p3.yzx + 33.33);
  return fract((p3.xx + p3.yz) * p3.zy);
}

struct ParticleWaveUniforms {
  float time_sec;
  float has_image;
  float image_w;
  float image_h;
  float phase_mode;
  float phase_progress;
  uint grid_w;
  uint grid_h;
  float viewport_w;
  float viewport_h;
  float tile_size;
  float fit_cover;
  float image_brightness;
  float image_peak_luminance;
  float image_saturation;
  float _pad0;
};

static float pw_value_noise(float2 p) {
  float2 i = floor(p);
  float2 f = fract(p);
  float2 u = f * f * (3.0 - 2.0 * f);
  float a = pw_hash22(i).x;
  float b = pw_hash22(i + float2(1.0, 0.0)).x;
  float c = pw_hash22(i + float2(0.0, 1.0)).x;
  float d = pw_hash22(i + float2(1.0, 1.0)).x;
  return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

static float pw_fbm(float2 p) {
  float v = 0.0;
  float a = 0.52;
  for (int i = 0; i < 3; ++i) {
    v += a * pw_value_noise(p);
    p = p * 2.07 + float2(13.7, 29.3);
    a *= 0.48;
  }
  return v;
}

static float pw_dissolve_threshold_px(float2 posPx, float2 cell, float2 res, bool fromTopLeft) {
  float2 uv = posPx / max(res, float2(1.0));
  float rawDiag = 0.58 * uv.x + 0.42 * uv.y;
  float diag = clamp(fromTopLeft ? rawDiag : (1.0 - rawDiag), 0.0, 1.0);
  float flake = pw_fbm(posPx * 0.006 + float2(2.3, 5.1));
  float subFlake = pw_value_noise(posPx * 0.023 + float2(11.4, 7.2));
  float grain = pw_hash22(cell + float2(3.1, 7.7)).x;
  return clamp(diag * 0.62 + flake * 0.24 + subFlake * 0.09 + grain * 0.05, 0.0, 1.0);
}

static float2 pw_wind_velocity_px(float2 posPx, float dirSign) {
  float n1 = pw_fbm(posPx * 0.0027 + float2(4.1, 8.7));
  float n2 = pw_fbm(posPx * 0.0037 + float2(31.7, 17.3));
  float baseAngle = dirSign > 0.0 ? atan2(-0.56, -0.83) : atan2(0.56, 0.83);
  float angle = baseAngle + (n1 - 0.5) * 0.72;
  float speed = 0.68 + n2 * 0.78;
  return float2(cos(angle), sin(angle)) * speed;
}

static float4 pw_sample_cell_image(
    float2 centerPx,
    float2 res,
    float2 imgSize,
    float fitCover,
    texture2d<float> imgTex,
    sampler imgSamp) {
  float imgAspect = imgSize.x / max(imgSize.y, 1.0);
  float viewAspect = res.x / max(res.y, 1.0);
  float drawW;
  float drawH;
  float2 origin;
  bool heightFit = imgAspect <= viewAspect;
  if (heightFit) {
    drawH = res.y;
    drawW = drawH * imgAspect;
    origin = float2(res.x - drawW, 0.0);
  } else {
    drawW = res.x;
    drawH = drawW / max(imgAspect, 0.001);
    origin = float2(0.0, (res.y - drawH) * 0.5);
  }
  float2 uv = (centerPx - origin) / float2(drawW, drawH);
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
    return float4(0.0);
  }
  float4 col = imgTex.sample(imgSamp, uv);
  if (fitCover > 0.5) {
    if (heightFit && origin.x > 1.0) {
      col.a *= smoothstep(0.0, 0.06, uv.x);
    } else if (!heightFit && origin.y > 1.0) {
      col.a *= smoothstep(0.0, 0.06, uv.y) * smoothstep(0.0, 0.06, 1.0 - uv.y);
    }
  }
  return col;
}

static uint pw_pack_tile(float3 rgb, float alpha) {
  uint a = uint(clamp(alpha, 0.0, 1.0) * 255.0 + 0.5);
  uint r = uint(clamp(rgb.r, 0.0, 1.0) * 255.0 + 0.5);
  uint g = uint(clamp(rgb.g, 0.0, 1.0) * 255.0 + 0.5);
  uint b = uint(clamp(rgb.b, 0.0, 1.0) * 255.0 + 0.5);
  return (a << 24) | (r << 16) | (g << 8) | b;
}

kernel void particle_tile_scatter_prep_kernel(
    texture2d<half, access::write> scatter_meta_tex [[texture(0)]],
    texture2d<half, access::write> scatter_vel_tex [[texture(1)]],
    constant ParticleWaveUniforms *uniforms [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= uniforms->grid_w || gid.y >= uniforms->grid_h) {
    return;
  }
  float tileSize = max(round(uniforms->tile_size), 2.0f);
  float2 src = float2(float(gid.x), float(gid.y));
  float2 srcPx = (src + 0.5f) * tileSize;
  float2 res = float2(uniforms->viewport_w, uniforms->viewport_h);
  bool is_disintegrate = uniforms->phase_mode > 0.5f && uniforms->phase_mode < 1.5f;

  float2 fineH = pw_hash22(src + (is_disintegrate ? float2(19.7f, 53.3f) : float2(29.1f, 47.9f)));
  float chunkNoise = pw_fbm(srcPx * 0.0075f + (is_disintegrate ? float2(8.3f, 14.1f) : float2(15.7f, 6.9f)));
  float threshold = pw_dissolve_threshold_px(srcPx, src, res, is_disintegrate);
  float dur = 0.28f + chunkNoise * 0.21f + fineH.x * 0.13f;
  float2 v1 = pw_wind_velocity_px(srcPx, is_disintegrate ? 1.0f : -1.0f);

  scatter_meta_tex.write(half4(half(threshold), half(1.0f / dur), half(chunkNoise), half(fineH.x)), gid);
  scatter_vel_tex.write(half4(half(v1.x), half(v1.y), half(fineH.y), 0.0h), gid);
}

kernel void particle_tile_scatter_kernel(
    device atomic_uint *tile_grid [[buffer(0)]],
    constant ParticleWaveUniforms *uniforms [[buffer(1)]],
    texture2d<float> backdrop_img [[texture(0)]],
    texture2d<half, access::read> scatter_meta_tex [[texture(1)]],
    texture2d<half, access::read> scatter_vel_tex [[texture(2)]],
    sampler backdrop_img_samp [[sampler(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= uniforms->grid_w || gid.y >= uniforms->grid_h) {
    return;
  }
  float mode = uniforms->phase_mode;
  bool is_disintegrate = mode > 0.5f && mode < 1.5f;
  bool is_reassemble = mode > 2.5f;
  if (!is_disintegrate && !is_reassemble) {
    return;
  }

  half4 meta = scatter_meta_tex.read(gid);
  float front = uniforms->phase_progress * 1.64f;
  float u = is_disintegrate
      ? (front - float(meta.x)) * float(meta.y)
      : (float(meta.x) - front) * float(meta.y) + 1.0f;
  if (u <= 0.0f || u >= 1.0f) {
    return;
  }

  float tileSize = max(round(uniforms->tile_size), 2.0f);
  float2 src = float2(float(gid.x), float(gid.y));
  float2 srcPx = (src + 0.5f) * tileSize;
  float2 res = float2(uniforms->viewport_w, uniforms->viewport_h);
  float2 imgSize = float2(uniforms->image_w, uniforms->image_h);

  float4 s = pw_sample_cell_image(srcPx, res, imgSize, uniforms->fit_cover, backdrop_img, backdrop_img_samp);
  if (s.a <= 0.05f) {
    return;
  }

  float peakCap = clamp(uniforms->image_peak_luminance, 0.0f, 1.0f);
  if (peakCap <= 0.001f) {
    return;
  }

  half4 vel = scatter_vel_tex.read(gid);
  float chunkNoise = float(meta.z);
  float2 fineH = float2(float(meta.w), float(vel.z));
  float2 v1 = float2(float(vel.x), float(vel.y));
  float dirSign = is_disintegrate ? 1.0f : -1.0f;

  float travelPx = pow(u, 1.56f) * (372.0f + chunkNoise * 240.0f);
  float2 midPx = srcPx + v1 * (travelPx * 0.45f);
  float2 v2 = pw_wind_velocity_px(midPx, dirSign);
  float2 flowPx = midPx + v2 * (travelPx * 0.55f);

  float crumblePx = (u * u) * (34.0f + chunkNoise * 42.0f);
  float2 grainOffsetPx = (fineH - 0.5f) * 2.0f * crumblePx;
  float2 posPx = flowPx + grainOffsetPx;
  int2 dst = int2(floor(posPx / tileSize));

  if (dst.x < 0 || dst.y < 0 || uint(dst.x) >= uniforms->grid_w || uint(dst.y) >= uniforms->grid_h) {
    return;
  }

  float luma = dot(s.rgb, float3(0.299f, 0.587f, 0.114f));
  float ashLift = is_disintegrate ? 0.06f : 0.08f;
  float ashAmt = is_disintegrate ? 0.72f : 0.65f;
  float3 outRgb = mix(s.rgb, float3(luma * 0.78f + ashLift), smoothstep(0.08f, 0.72f, u) * ashAmt);
  float outAlpha = s.a * smoothstep(1.0f, is_disintegrate ? 0.18f : 0.14f, u);
  if (outAlpha <= 0.03f) {
    return;
  }

  float imgLuma = dot(outRgb, float3(0.299f, 0.587f, 0.114f));
  float imgSat = clamp(uniforms->image_saturation, 0.0f, 1.5f);
  float3 imgAdj = max(mix(float3(imgLuma), outRgb, imgSat), float3(0.0f));
  imgAdj *= max(uniforms->image_brightness, 0.0f);
  float imgPeak = max(imgAdj.r, max(imgAdj.g, imgAdj.b));
  if (imgPeak > 1e-4f) {
    float mapped = peakCap * (1.0f - fast::exp(-imgPeak / peakCap));
    imgAdj *= (mapped / imgPeak);
  }
  float3 normRgb = imgAdj / peakCap;

  uint idx = uint(dst.y) * uniforms->grid_w + uint(dst.x);
  uint packed = pw_pack_tile(normRgb, outAlpha);
  atomic_fetch_max_explicit(&tile_grid[idx], packed, memory_order_relaxed);
}

struct FacetLutUniforms {
  uint grid_w;
  uint grid_h;
  float2 _pad0;
  float4 col_blue;
  float4 col_violet;
  float4 col_cyan;
};

kernel void particle_facet_lut_kernel(
    texture2d<half, access::write> facet_lut_tex [[texture(0)]],
    constant FacetLutUniforms &u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= u.grid_w || gid.y >= u.grid_h) {
    return;
  }
  float2 h = pw_hash22(float2(float(gid.x), float(gid.y)));
  float3 facetColor = mix(u.col_blue.rgb, u.col_violet.rgb, h.y);
  facetColor = mix(facetColor, u.col_cyan.rgb, smoothstep(0.65, 1.0, h.y) * 0.6);
  facet_lut_tex.write(half4(half3(saturate(facetColor)), half(saturate(h.x))), gid);
}

kernel void particle_tile_hold_kernel(
    texture2d<half, access::write> tile_img_tex [[texture(0)]],
    constant ParticleWaveUniforms *uniforms [[buffer(0)]],
    texture2d<float> backdrop_img [[texture(1)]],
    sampler backdrop_img_samp [[sampler(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= uniforms->grid_w || gid.y >= uniforms->grid_h) {
    return;
  }
  float peakCap = clamp(uniforms->image_peak_luminance, 0.0, 1.0);
  if (peakCap <= 0.001) {
    tile_img_tex.write(half4(0.0h), gid);
    return;
  }
  float tileSize = max(round(uniforms->tile_size), 2.0);
  float2 srcPx = (float2(float(gid.x), float(gid.y)) + 0.5) * tileSize;
  float2 res = float2(uniforms->viewport_w, uniforms->viewport_h);
  float2 imgSize = float2(uniforms->image_w, uniforms->image_h);
  float4 s = pw_sample_cell_image(srcPx, res, imgSize, uniforms->fit_cover, backdrop_img, backdrop_img_samp);
  if (s.a <= 0.05) {
    tile_img_tex.write(half4(0.0h), gid);
    return;
  }
  float imgLuma = dot(s.rgb, float3(0.299, 0.587, 0.114));
  float imgSat = clamp(uniforms->image_saturation, 0.0, 1.5);
  float3 imgAdj = max(mix(float3(imgLuma), s.rgb, imgSat), float3(0.0));
  imgAdj *= max(uniforms->image_brightness, 0.0);
  float imgPeak = max(imgAdj.r, max(imgAdj.g, imgAdj.b));
  if (imgPeak > 1e-4) {
    float mapped = peakCap * (1.0 - exp(-imgPeak / peakCap));
    imgAdj *= (mapped / imgPeak);
  }
  float3 normRgb = saturate(imgAdj / peakCap);
  tile_img_tex.write(half4(half3(normRgb), half(saturate(s.a))), gid);
}

kernel void particle_tile_unpack_to_tex_kernel(
    device uint *tile_grid [[buffer(0)]],
    texture2d<half, access::write> tile_img_tex [[texture(0)]],
    constant ParticleWaveUniforms *uniforms [[buffer(1)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= uniforms->grid_w || gid.y >= uniforms->grid_h) {
    return;
  }
  uint idx = gid.y * uniforms->grid_w + gid.x;
  uint packed = tile_grid[idx];
  tile_grid[idx] = 0u;
  if (packed == 0u) {
    tile_img_tex.write(half4(0.0h), gid);
  } else {
    half4 v = half4(
        half((packed >> 16) & 0xFFu),
        half((packed >> 8) & 0xFFu),
        half(packed & 0xFFu),
        half((packed >> 24) & 0xFFu)) * (1.0h / 255.0h);
    tile_img_tex.write(v, gid);
  }
}

constant bool kWaveHasImage [[function_constant(0)]];

struct LiveVideoBufferInfo {
  uint width;
  uint height;
  uint bytes_per_pixel;
  uint has_alpha;
};

static inline float4 sample_live_video_linear(
    device const uchar *pixels,
    uint src_w,
    uint src_h,
    uint bpp,
    uint has_alpha,
    float2 uv) {
  float fx = clamp(uv.x * float(src_w) - 0.5f, 0.0f, float(src_w - 1u));
  float fy = clamp(uv.y * float(src_h) - 0.5f, 0.0f, float(src_h - 1u));
  uint x0 = uint(fx);
  uint y0 = uint(fy);
  uint x1 = min(x0 + 1u, src_w - 1u);
  uint y1 = min(y0 + 1u, src_h - 1u);
  float tx = fx - float(x0);
  float ty = fy - float(y0);

  if (bpp == 4u) {
    device const uchar4 *px4 = reinterpret_cast<device const uchar4 *>(pixels);
    float4 p00 = float4(px4[y0 * src_w + x0]);
    float4 p10 = float4(px4[y0 * src_w + x1]);
    float4 p01 = float4(px4[y1 * src_w + x0]);
    float4 p11 = float4(px4[y1 * src_w + x1]);
    float4 rgba = mix(mix(p00, p10, tx), mix(p01, p11, tx), ty) * (1.0f / 255.0f);
    return float4(rgba.rgb, (has_alpha != 0u) ? rgba.a : 1.0f);
  }

  uint i00 = (y0 * src_w + x0) * 3u;
  uint i10 = (y0 * src_w + x1) * 3u;
  uint i01 = (y1 * src_w + x0) * 3u;
  uint i11 = (y1 * src_w + x1) * 3u;

  float3 c00 = float3(pixels[i00], pixels[i00 + 1u], pixels[i00 + 2u]);
  float3 c10 = float3(pixels[i10], pixels[i10 + 1u], pixels[i10 + 2u]);
  float3 c01 = float3(pixels[i01], pixels[i01 + 1u], pixels[i01 + 2u]);
  float3 c11 = float3(pixels[i11], pixels[i11 + 1u], pixels[i11 + 2u]);
  float3 rgb = mix(mix(c00, c10, tx), mix(c01, c11, tx), ty) * (1.0f / 255.0f);
  return float4(rgb, 1.0f);
}

static inline half4 compute_shaded_tile(
    uint2 gid,
    constant ParticleWaveUniforms *uniforms,
    half4 facet,
    bool has_img,
    half4 img) {
  float tileSize = max(round(uniforms->tile_size), 2.0f);
  float subpixelShift = (tileSize <= 2.5f) ? 0.35f : 0.0f;
  float2 fragPx = (float2(gid) + 0.5f) * tileSize - float2(subpixelShift);
  float t = uniforms->time_sec;
  float p1 = fma(fragPx.x, 0.00185f, fma(fragPx.y, 0.00105f, t * -0.6139f));
  float p2 = fma(fragPx.x, 0.00120f, fma(fragPx.y, -0.00195f, t * 0.44185f));
  float wave = fma(fast::sin(p1), 0.58f, fast::cos(p2) * 0.42f);

  half hx = facet.a;
  half align = saturate(fast::cos(half(wave * 1.25f) + (hx - 0.5h) * 1.35h));
  half a2 = align * align;
  half a3 = a2 * align;
  half a4 = a2 * a2;
  half a10 = a4 * a4 * a2;
  half softGlow = saturate(half((wave + 0.25f) * (1.0f / 1.15f)));
  softGlow = softGlow * softGlow * (3.0h - 2.0h * softGlow);
  half glint = a3 * fma(softGlow, 0.80h, 0.20h);
  half sharpSpark = a10 * softGlow;

  half3 bevel_rgb = facet.rgb * fma(softGlow, 0.045h, glint * 0.14h);
  half3 flat_extra = half3(0.010h, 0.014h, 0.022h) * fma(hx, 0.5h, 0.5h);

  if (has_img) {
    half img_scale = half(clamp(uniforms->image_peak_luminance, 0.0f, 1.0f) * 0.85f);
    half facetShade = fma(hx, 0.45h, 0.65h);
    half waveMod = fma(softGlow, 0.70h, fma(glint, 0.85h, 0.30h));
    half alphaScale = img.a * img_scale;
    bevel_rgb = fma(img.rgb, half3(facetShade * waveMod * alphaScale), bevel_rgb);
    flat_extra = fma(img.rgb, half3(sharpSpark * 0.28h * alphaScale), flat_extra);
  }

  bevel_rgb += flat_extra * (1.0h / 0.97h);
  return half4(bevel_rgb, sharpSpark);
}

kernel void particle_tile_unpack_shade_kernel(
    device uint *tile_grid [[buffer(0)]],
    texture2d<half, access::read> facet_lut_tex [[texture(0)]],
    texture2d<half, access::write> tile_lit_tex [[texture(1)]],
    texture2d<half, access::read> scatter_meta_tex [[texture(2)]],
    texture2d<half, access::read> tile_img_tex [[texture(3)]],
    constant ParticleWaveUniforms *uniforms [[buffer(1)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= uniforms->grid_w || gid.y >= uniforms->grid_h) {
    return;
  }
  uint idx = gid.y * uniforms->grid_w + gid.x;
  uint packed = tile_grid[idx];
  tile_grid[idx] = 0u;
  half4 img = half4(0.0h);
  if (packed != 0u) {
    img = half4(
        half((packed >> 16) & 0xFFu),
        half((packed >> 8) & 0xFFu),
        half(packed & 0xFFu),
        half((packed >> 24) & 0xFFu)) * (1.0h / 255.0h);
  }
  half4 meta = scatter_meta_tex.read(gid);
  float front = uniforms->phase_progress * 1.64f;
  bool is_disintegrate = uniforms->phase_mode > 0.5f && uniforms->phase_mode < 1.5f;
  float u = is_disintegrate
      ? (front - float(meta.x)) * float(meta.y)
      : (float(meta.x) - front) * float(meta.y) + 1.0f;
  if (u <= 0.0f) {
    half4 static_img = tile_img_tex.read(gid);
    if (static_img.a > img.a || (static_img.a == img.a && static_img.r > img.r)) {
      img = static_img;
    }
  }
  half4 facet = facet_lut_tex.read(gid);
  tile_lit_tex.write(compute_shaded_tile(gid, uniforms, facet, true, img), gid);
}

kernel void particle_tile_shade_kernel(
    texture2d<half, access::read> facet_lut_tex [[texture(0)]],
    texture2d<half, access::read> tile_img_tex [[texture(1)]],
    texture2d<half, access::write> tile_lit_tex [[texture(2)]],
    constant ParticleWaveUniforms *uniforms [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= uniforms->grid_w || gid.y >= uniforms->grid_h) {
    return;
  }
  half4 facet = facet_lut_tex.read(gid);
  half4 img = kWaveHasImage ? tile_img_tex.read(gid) : half4(0.0h);
  tile_lit_tex.write(compute_shaded_tile(gid, uniforms, facet, kWaveHasImage, img), gid);
}

kernel void particle_tile_live_buf_shade_kernel(
    texture2d<half, access::read> facet_lut_tex [[texture(0)]],
    texture2d<half, access::write> tile_lit_tex [[texture(1)]],
    constant ParticleWaveUniforms *uniforms [[buffer(0)]],
    device const uchar *live_pixels [[buffer(1)]],
    constant LiveVideoBufferInfo &info [[buffer(2)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= uniforms->grid_w || gid.y >= uniforms->grid_h) {
    return;
  }
  half4 facet = facet_lut_tex.read(gid);
  float peakCap = clamp(uniforms->image_peak_luminance, 0.0f, 1.0f);
  half4 img = half4(0.0h);
  if (peakCap > 0.001f && info.width > 0u && info.height > 0u) {
    float tileSize = max(round(uniforms->tile_size), 2.0f);
    float2 srcPx = (float2(float(gid.x), float(gid.y)) + 0.5f) * tileSize;
    float2 res = float2(uniforms->viewport_w, uniforms->viewport_h);
    float2 imgSize = float2(float(info.width), float(info.height));
    float imgAspect = imgSize.x / max(imgSize.y, 1.0f);
    float viewAspect = res.x / max(res.y, 1.0f);
    float drawW;
    float drawH;
    float2 origin;
    bool heightFit = imgAspect <= viewAspect;
    if (heightFit) {
      drawH = res.y;
      drawW = drawH * imgAspect;
      origin = float2(res.x - drawW, 0.0f);
    } else {
      drawW = res.x;
      drawH = drawW / max(imgAspect, 0.001f);
      origin = float2(0.0f, (res.y - drawH) * 0.5f);
    }
    float2 uv = (srcPx - origin) / float2(drawW, drawH);
    float4 s = float4(0.0f);
    if (uv.x >= 0.0f && uv.x <= 1.0f && uv.y >= 0.0f && uv.y <= 1.0f) {
      if (info.width == uniforms->grid_w && info.height == uniforms->grid_h &&
          abs(origin.x) < 1.0f && abs(origin.y) < 1.0f) {
        if (info.bytes_per_pixel == 4u) {
          device const uchar4 *px4 = reinterpret_cast<device const uchar4 *>(live_pixels);
          float4 p = float4(px4[gid.y * info.width + gid.x]) * (1.0f / 255.0f);
          s = float4(p.rgb, (info.has_alpha != 0u) ? p.a : 1.0f);
        } else {
          uint idx = (gid.y * info.width + gid.x) * 3u;
          float3 rgb = float3(live_pixels[idx], live_pixels[idx + 1u], live_pixels[idx + 2u]) * (1.0f / 255.0f);
          s = float4(rgb, 1.0f);
        }
      } else {
        s = sample_live_video_linear(
            live_pixels, info.width, info.height, info.bytes_per_pixel, info.has_alpha, uv);
      }
      if (uniforms->fit_cover > 0.5f) {
        if (heightFit && origin.x > 1.0f) {
          s.a *= smoothstep(0.0f, 0.06f, uv.x);
        } else if (!heightFit && origin.y > 1.0f) {
          s.a *= smoothstep(0.0f, 0.06f, uv.y) * smoothstep(0.0f, 0.06f, 1.0f - uv.y);
        }
      }
    }
    if (s.a > 0.05f) {
      float imgLuma = dot(s.rgb, float3(0.299f, 0.587f, 0.114f));
      float imgSat = clamp(uniforms->image_saturation, 0.0f, 1.5f);
      float3 imgAdj = max(mix(float3(imgLuma), s.rgb, imgSat), float3(0.0f));
      imgAdj *= max(uniforms->image_brightness, 0.0f);
      float imgPeak = max(imgAdj.r, max(imgAdj.g, imgAdj.b));
      if (imgPeak > 1e-4f) {
        float mapped = peakCap * (1.0f - fast::exp(-imgPeak / peakCap));
        imgAdj *= (mapped / imgPeak);
      }
      img = half4(half3(saturate(imgAdj / peakCap)), half(saturate(s.a)));
    }
  }
  tile_lit_tex.write(compute_shaded_tile(gid, uniforms, facet, true, img), gid);
}

struct ParticleWaveVertexOutput {
  float4 position [[position]];
  float2 grid_pos;
  half3 base_col [[flat]];
  half3 base_col_grout [[flat]];
  half inv_grout_tile [[flat]];
  half brightness [[flat]];
  uint2 max_cell [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct ParticleWaveFragmentInput {
  float4 position [[position]];
  float2 grid_pos;
  half3 base_col [[flat]];
  half3 base_col_grout [[flat]];
  half inv_grout_tile [[flat]];
  half brightness [[flat]];
  uint2 max_cell [[flat]];
};

vertex ParticleWaveVertexOutput particle_wave_vertex(
    uint unit_vertex_id [[vertex_id]], uint quad_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(QuadInputIndex_Vertices)]],
    constant Quad *quads [[buffer(QuadInputIndex_Quads)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(QuadInputIndex_ViewportSize)]],
    constant ParticleWaveUniforms *uniforms [[buffer(3)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Quad quad = quads[quad_id];

  float4 device_position =
      to_device_position(unit_vertex, quad.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, quad.bounds,
                                                 quad.content_mask.bounds);

  float2 fragPx = float2(quad.bounds.origin.x, quad.bounds.origin.y) +
                  unit_vertex * float2(quad.bounds.size.width, quad.bounds.size.height);
  float tileSize = max(round(uniforms->tile_size), 2.0);
  float2 subpixelShift = (tileSize <= 2.5f) ? float2(0.35f) : float2(0.0f);
  float2 grid_pos = (fragPx + subpixelShift) / tileSize;

  half3 baseCol = half3(hsla_to_rgba(quad.background.solid).rgb);
  half brightness = half(max(quad.background.gradient_angle_or_pattern_height, 0.0));
  half groutWidth = half(min(1.35f, tileSize * 0.24f));
  half inv_grout_tile = half(tileSize) / groutWidth;
  uint2 max_cell = uint2(max(uniforms->grid_w, 1u) - 1u, max(uniforms->grid_h, 1u) - 1u);

  return ParticleWaveVertexOutput{
      device_position,
      grid_pos,
      baseCol,
      baseCol * 0.86h,
      inv_grout_tile,
      brightness,
      max_cell,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment half4 particle_wave_fragment(
    ParticleWaveFragmentInput input [[stage_in]],
    texture2d<half, access::read> tile_lit_tex [[texture(0)]]) {
  float2 gridPos = input.grid_pos;
  float2 cell = floor(gridPos);
  half2 local = half2(gridPos - cell);

  half2 edgeFrac = min(local, half2(1.0h) - local);
  half groutT = saturate(min(edgeFrac.x, edgeFrac.y) * input.inv_grout_tile);
  half tileMask = groutT * groutT * (3.0h - 2.0h * groutT);

  uint2 cellU = min(uint2(max(cell, float2(0.0f))), input.max_cell);
  half4 lit = tile_lit_tex.read(cellU);
  half bevel = 1.06h - 0.12h * local.y - 0.06h * local.x;
  half3 tileRgb = fma(lit.rgb, half3(bevel),
                      fma(half3(0.072h, 0.082h, 0.096h), half3(lit.a), input.base_col));
  half3 finalCol = mix(input.base_col_grout, tileRgb, tileMask) * input.brightness;
  return half4(saturate(finalCol), 1.0h);
}

static float4 sample_backdrop_frost(
    texture2d<float> backdrop_tex,
    sampler backdrop_samp,
    float2 uvCoord,
    float blurPx,
    float2 res,
    float glassMask) {
  if (blurPx <= 0.5) {
    return backdrop_tex.sample(backdrop_samp, uvCoord, level(0.0));
  }
  float radiusPx = blurPx * glassMask;
  float maxLod = max(float(backdrop_tex.get_num_mip_levels()) - 1.0, 0.0);
  // Sample from a hardware-prefiltered mip level matched to the blur radius so
  // neighbor taps always overlap, eliminating grid/ghost aliasing on sharp text.
  float lod = clamp(log2(max(radiusPx * 0.55, 1.0)), 0.0, maxLod);
  float2 bOff = radiusPx / res;
  float2 innerOff = bOff * 0.45;
  float2 diagOff = bOff * 0.7071;
  float2 uvMin = float2(0.0005);
  float2 uvMax = float2(0.9995);

  float4 c0 = backdrop_tex.sample(backdrop_samp, uvCoord, level(lod));

  float4 i0 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(innerOff.x, innerOff.y), uvMin, uvMax), level(lod));
  float4 i1 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(-innerOff.x, innerOff.y), uvMin, uvMax), level(lod));
  float4 i2 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(innerOff.x, -innerOff.y), uvMin, uvMax), level(lod));
  float4 i3 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(-innerOff.x, -innerOff.y), uvMin, uvMax), level(lod));

  float4 o0 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(bOff.x, 0.0), uvMin, uvMax), level(lod));
  float4 o1 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord - float2(bOff.x, 0.0), uvMin, uvMax), level(lod));
  float4 o2 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(0.0, bOff.y), uvMin, uvMax), level(lod));
  float4 o3 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord - float2(0.0, bOff.y), uvMin, uvMax), level(lod));

  float4 d0 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(diagOff.x, diagOff.y), uvMin, uvMax), level(lod));
  float4 d1 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(-diagOff.x, diagOff.y), uvMin, uvMax), level(lod));
  float4 d2 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(diagOff.x, -diagOff.y), uvMin, uvMax), level(lod));
  float4 d3 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(-diagOff.x, -diagOff.y), uvMin, uvMax), level(lod));

  return c0 * 0.16
       + (i0 + i1 + i2 + i3) * 0.11
       + (o0 + o1 + o2 + o3) * 0.06
       + (d0 + d1 + d2 + d3) * 0.04;
}

fragment float4 liquid_glass_fragment(
    QuadFragmentInput input [[stage_in]],
    constant Quad *quads [[buffer(QuadInputIndex_Quads)]],
    constant Size_DevicePixels *viewport_size [[buffer(QuadInputIndex_ViewportSize)]],
    constant float *time_sec [[buffer(3)]],
    texture2d<float> backdrop_tex [[texture(0)]],
    sampler backdrop_samp [[sampler(0)]]) {
  Quad quad = quads[input.quad_id];
  float2 fragPx = input.position.xy;
  float2 psize = float2(quad.bounds.size.width, quad.bounds.size.height);
  float2 phalf = psize * 0.5;
  float2 pcenter = float2(quad.bounds.origin.x, quad.bounds.origin.y) + phalf;

  float corner_radius = pick_corner_radius(fragPx - pcenter, quad.corner_radii);
  float r = min(corner_radius, min(phalf.x, phalf.y));

  // SDF of rounded rectangle (negative inside, positive outside)
  float2 q = abs(fragPx - pcenter) - phalf + r;
  float d = min(max(q.x, q.y), 0.0) + length(max(q, float2(0.0))) - r;
  float glassMask = 1.0 - smoothstep(-1.0, 1.0, d);
  if (glassMask <= 0.001) {
    discard_fragment();
  }

  // Unpack Voxel World Agents (overworld-agents) Liquid Glass parameters (CSS px -> 2x device px)
  float4 tint = input.background_solid;
  float dispScale = quad.background.gradient_angle_or_pattern_height * 2.0;
  float refractReach = quad.background.colors[0].percentage * 2.0;
  float chroma = quad.background.colors[0].color.s;
  float highlightAlpha = quad.background.colors[0].color.a;
  float edgeMode = quad.background.colors[1].color.h;
  float edgeTintKind = quad.background.colors[1].color.s;
  float rimWidth = quad.background.colors[1].color.a * 2.0;
  float blurPx = quad.background.colors[1].percentage * 2.0;

  // Squircle dome refraction (edge wrap-around / "潜り込み" displacement)
  // Interior pixels beyond `reach` have xDepth == 1 -> bend == 0, so skip heavy trig/exp/sqrt.
  float reach = clamp(refractReach, 1.0, max(min(phalf.x, phalf.y), 1.0));
  float bend = 0.0;
  float2 dispPx = float2(0.0);
  if (-d < reach && dispScale > 0.001) {
    float2 rel = fragPx - pcenter;
    float qx = abs(rel.x) - phalf.x + r;
    float qy = abs(rel.y) - phalf.y + r;
    float qm = max(qx, qy);
    float ex = exp((qx - qm) / reach);
    float ey = exp((qy - qm) / reach);
    float2 gradN = normalize(float2((rel.x >= 0.0 ? 1.0 : -1.0) * ex,
                                    (rel.y >= 0.0 ? 1.0 : -1.0) * ey) + float2(1e-5));
    float xDepth = clamp(-d / reach, 0.0, 1.0);
    float omx = 1.0 - xDepth;
    float omx3 = omx * omx * omx;
    float omx4 = omx3 * omx;
    float denom = max(1.0 - omx4, 1e-4);
    float slope = omx3 / (sqrt(denom) * sqrt(sqrt(denom)));
    float inv = rsqrt(slope * slope + 1.0);
    float sinI = slope * inv;
    float cosI = inv;
    float sinT = sinI * (1.0 / 1.5);
    float cosT = sqrt(max(1.0 - sinT * sinT, 0.0));
    bend = sinI * cosT - cosI * sinT;
    dispPx = -gradN * (bend * dispScale) * glassMask;
  }

  float2 res = max(float2((float)viewport_size->width, (float)viewport_size->height), float2(1.0));
  float2 refractUV = clamp((fragPx + dispPx) / res, float2(0.0005), float2(0.9995));
  float4 bgSample = sample_backdrop_frost(backdrop_tex, backdrop_samp, refractUV, blurPx, res, glassMask);

  if (chroma > 0.001 && bend > 0.0001) {
    float k = chroma * 0.6;
    float2 rUV = clamp((fragPx + dispPx * (1.0 - k)) / res, float2(0.0005), float2(0.9995));
    float2 bUV = clamp((fragPx + dispPx * (1.0 + k)) / res, float2(0.0005), float2(0.9995));
    float rCh = sample_backdrop_frost(backdrop_tex, backdrop_samp, rUV, blurPx, res, glassMask).r;
    float bCh = sample_backdrop_frost(backdrop_tex, backdrop_samp, bUV, blurPx, res, glassMask).b;
    bgSample.rgb = float3(rCh, bgSample.g, bCh);
  }

  float bgVis = clamp(bgSample.a, 0.0, 1.0);
  float3 backdropRgb = bgVis > 0.001 ? bgSample.rgb : tint.rgb;
  float3 col = mix(backdropRgb, tint.rgb, tint.a);

  // Minimal edge caustic so refraction stays crisp without milky white haze
  col += float3(0.72, 0.86, 1.0) * (bend * 0.015 * glassMask);

  float specular = 0.0;
  float edgeGlow = 0.0;
  if (edgeMode > 0.12 && edgeMode < 0.38) {
    // Plan A: Pure optical fresnel rim (no flat uniform border; top-left key-light sheen)
    float2 edgeDir = normalize(fragPx - pcenter + float2(1e-5));
    float keyDir = clamp(0.62 - edgeDir.y * 0.42 - edgeDir.x * 0.22, 0.10, 1.0);
    float innerRim = (1.0 - smoothstep(0.0, 4.0, -d)) * step(d, 0.0);
    float microCaustic = (1.0 - smoothstep(1.5, 10.0, -d)) * step(d, 0.0) * 0.35;
    float3 rimCol = float3(0.88, 0.93, 1.0);
    float strength = 0.14;
    if (edgeTintKind > 0.75) {
      rimCol = float3(0.95, 0.72, 0.28);
      strength = 0.30;
    } else if (edgeTintKind > 0.25) {
      rimCol = float3(0.45, 0.68, 1.0);
      strength = 0.30;
    }
    edgeGlow = (innerRim + microCaustic) * keyDir * strength;
    col = mix(col, rimCol, edgeGlow * glassMask);
  } else if (edgeMode >= 0.38 && edgeMode < 0.62) {
    // Plan B: Directional top-fade hairline (1px cut-glass top edge fading down sides)
    float topY = pcenter.y - phalf.y;
    float yNorm = clamp((fragPx.y - topY) / max(psize.y * 0.85, 1.0), 0.0, 1.0);
    float topFade = (1.0 - yNorm);
    topFade = topFade * topFade;
    float hairline = (1.0 - smoothstep(0.0, 2.2, -d)) * step(d, 0.0);
    float3 rimCol = float3(0.90, 0.94, 1.0);
    float strength = 0.28;
    if (edgeTintKind > 0.75) {
      rimCol = float3(0.95, 0.72, 0.28);
      strength = 0.62;
    } else if (edgeTintKind > 0.25) {
      rimCol = float3(0.42, 0.64, 1.0);
      strength = 0.62;
    }
    edgeGlow = hairline * topFade * strength;
    col = mix(col, rimCol, edgeGlow * glassMask);
  } else if (edgeMode >= 0.62 && edgeMode < 0.88) {
    // Plan C: Borderless (refraction + subtle tonal plane only)
    edgeGlow = 0.0;
  } else if (edgeMode >= 0.88 && edgeMode < 1.12) {
    // Plan D: Plan B hairline + dynamic wave-driven moving light source synced to backdrop wave
    float t = *time_sec;
    float2 wavePos = fragPx + float2(t * 5.0, -t * 3.0);
    float p1 = wavePos.x * 0.00185 + wavePos.y * 0.00105 - t * 0.62;
    float p2 = wavePos.x * 0.00120 - wavePos.y * 0.00195 + t * 0.43;
    float wave = sin(p1) * 0.58 + cos(p2) * 0.42;
    float waveCrest = smoothstep(-0.45, 0.90, wave);

    float2 waveGrad = normalize(
      float2(0.00185, 0.00105) * (cos(p1) * 0.58)
      - float2(0.00120, -0.00195) * (sin(p2) * 0.42)
      + float2(1e-5)
    );
    float2 edgeDir = normalize(fragPx - pcenter + float2(1e-5));
    float dirHit = clamp(dot(edgeDir, waveGrad) * 0.5 + 0.5, 0.0, 1.0);
    dirHit = dirHit * dirHit;

    float topY = pcenter.y - phalf.y;
    float yNorm = clamp((fragPx.y - topY) / max(psize.y * 0.9, 1.0), 0.0, 1.0);
    float topFade = (1.0 - yNorm) * (1.0 - yNorm);

    float hairline = (1.0 - smoothstep(0.0, max(rimWidth * 1.35, 1.6), -d)) * step(d, 0.0);
    float specCtrl = clamp(highlightAlpha, 0.0, 1.0);
    float3 rimCol = mix(float3(0.76, 0.86, 1.0), float3(0.95, 0.98, 1.0), waveCrest);
    float baseOutline = 0.10;
    float dynamicStrength = 0.16 + specCtrl * 0.52;
    if (edgeTintKind > 0.75) {
      rimCol = mix(float3(0.95, 0.72, 0.28), float3(1.0, 0.88, 0.62), waveCrest * 0.5);
      baseOutline = 0.24;
      dynamicStrength = 0.36 + specCtrl * 0.54;
    } else if (edgeTintKind > 0.25) {
      rimCol = mix(float3(0.42, 0.64, 1.0), float3(0.78, 0.90, 1.0), waveCrest * 0.55);
      baseOutline = 0.24;
      dynamicStrength = 0.36 + specCtrl * 0.54;
    }
    float dynamicEnv = topFade * 0.38 + dirHit * (0.30 + 0.75 * waveCrest);
    edgeGlow = hairline * clamp(baseOutline + dynamicEnv * dynamicStrength, 0.0, 1.0);
    col = mix(col, rimCol, edgeGlow * glassMask);
  } else if (edgeMode >= 1.12) {
    // Crisp Edge Patterns 1..4 (edgeMode = 1.25, 1.50, 1.75, 2.00)
    float t = *time_sec;
    float2 wavePos = fragPx + float2(t * 5.0, -t * 3.0);
    float p1 = wavePos.x * 0.00185 + wavePos.y * 0.00105 - t * 0.62;
    float p2 = wavePos.x * 0.00120 - wavePos.y * 0.00195 + t * 0.43;
    float wave = sin(p1) * 0.58 + cos(p2) * 0.42;
    float waveCrest = smoothstep(-0.35, 0.90, wave);
    float2 waveGrad = normalize(
      float2(0.00185, 0.00105) * (cos(p1) * 0.58)
      - float2(0.00120, -0.00195) * (sin(p2) * 0.42)
      + float2(1e-5)
    );
    float2 edgeDir = normalize(fragPx - pcenter + float2(1e-5));
    float dirRaw = clamp(dot(edgeDir, waveGrad) * 0.5 + 0.5, 0.0, 1.0);
    float topY = pcenter.y - phalf.y;
    float yNorm = clamp((fragPx.y - topY) / max(psize.y * 0.85, 1.0), 0.0, 1.0);
    float topFade = (1.0 - yNorm) * (1.0 - yNorm);

    // Razor-sharp 1.15 device px outer cutline (~0.55 CSS px)
    float razor = (1.0 - smoothstep(0.0, 1.25, -d)) * step(d, 0.0);

    if (edgeMode < 1.38) {
      // Pattern 1: Razor Platinum (screen/additive cold platinum, zero mud)
      float glint = topFade * 0.35 + (dirRaw * dirRaw) * (0.25 + 0.75 * waveCrest);
      float3 tintC = float3(0.88, 0.91, 0.96);
      float amp = 0.24;
      if (edgeTintKind > 0.75) {
        tintC = float3(0.95, 0.78, 0.46);
        amp = 0.44;
      } else if (edgeTintKind > 0.25) {
        tintC = float3(0.66, 0.80, 1.0);
        amp = 0.44;
      }
      float e = razor * (0.14 + glint * amp);
      col = col + tintC * e * (1.0 - col) * glassMask;
      edgeGlow = e;
    } else if (edgeMode < 1.62) {
      // Pattern 2: Ice Chrome Dual-Bevel (bright 1px outer razor + dark 1.5px inner undercut)
      float undercut = smoothstep(0.9, 1.5, -d) * (1.0 - smoothstep(1.5, 3.2, -d)) * step(d, 0.0);
      col *= (1.0 - undercut * 0.42 * glassMask);
      float glint = topFade * 0.45 + (dirRaw * dirRaw) * (0.25 + 0.75 * waveCrest);
      float3 tintC = float3(0.84, 0.90, 0.98);
      float amp = 0.32;
      if (edgeTintKind > 0.75) {
        tintC = float3(0.98, 0.76, 0.36);
        amp = 0.56;
      } else if (edgeTintKind > 0.25) {
        tintC = float3(0.52, 0.74, 1.0);
        amp = 0.56;
      }
      float e = razor * (0.18 + glint * amp);
      col = col + tintC * e * (1.0 - col * 0.65) * glassMask;
      edgeGlow = e;
    } else if (edgeMode < 1.88) {
      // Pattern 3: Backdrop-Synced (steals backdrop wave hue; selected/gate inherit tint hue)
      float3 syncHue = clamp(backdropRgb * 2.8 + float3(0.10), 0.0, 1.0);
      float3 tintHue = clamp(tint.rgb * 3.2 + float3(0.28), 0.0, 1.0);
      float3 tintC = mix(syncHue, float3(0.92, 0.94, 0.98), 0.28);
      float specCtrl = clamp(highlightAlpha, 0.0, 1.0);
      float amp = 0.16 + specCtrl * 0.56;
      if (edgeTintKind > 0.75) {
        tintC = mix(syncHue, tintHue, 0.72);
        amp = 0.28 + specCtrl * 0.68;
      } else if (edgeTintKind > 0.25) {
        tintC = mix(syncHue, tintHue, 0.70);
        amp = 0.28 + specCtrl * 0.68;
      }
      float glint = topFade * 0.30 + (dirRaw * dirRaw * dirRaw) * (0.30 + 0.85 * waveCrest);
      float e = razor * (0.10 + glint * amp);
      col = col + tintC * e * glassMask;
      edgeGlow = e;
    } else {
      // Pattern 4: Stealth Obsidian (dark obsidian cut + razor white pinpoint specular glint)
      float darkRim = (1.0 - smoothstep(0.0, 2.2, -d)) * step(d, 0.0);
      col *= (1.0 - darkRim * 0.48 * glassMask);
      float sharpAngle = dirRaw * dirRaw;
      sharpAngle = sharpAngle * sharpAngle * sharpAngle; // pow 6 pinpoint glint
      float3 tintC = float3(0.92, 0.95, 1.0);
      float baseLine = 0.04;
      float glintAmp = 0.42;
      if (edgeTintKind > 0.75) {
        tintC = float3(0.96, 0.75, 0.36);
        baseLine = 0.18;
        glintAmp = 0.52;
      } else if (edgeTintKind > 0.25) {
        tintC = float3(0.58, 0.78, 1.0);
        baseLine = 0.18;
        glintAmp = 0.52;
      }
      float e = razor * (baseLine + (topFade * 0.18 + sharpAngle * (0.25 + 0.75 * waveCrest)) * glintAmp);
      col = col + tintC * e * glassMask;
      edgeGlow = e;
    }
  } else if (highlightAlpha > 0.001) {
    float rimLight = (1.0 - smoothstep(0.0, max(rimWidth, 0.5), -d)) * step(d, 0.0);
    float topY = pcenter.y - phalf.y;
    float keyBias = clamp(1.0 - smoothstep(0.0, phalf.y * 1.4, fragPx.y - topY), 0.0, 1.0);
    specular = clamp(rimLight * (0.55 + keyBias * 0.45), 0.0, 1.0);
    col = mix(col, float3(1.0), specular * highlightAlpha * glassMask);
  }

  float mask_fade = content_mask_fade(quad.content_mask, fragPx);
  float glassOpacity = saturate(1.0 - quad.background.colors[0].color.h);
  float outA = mix(tint.a, 1.0, bgVis) * glassMask;
  outA = clamp(max(outA, max(specular * highlightAlpha, edgeGlow) * glassMask), 0.0, 1.0) * mask_fade * glassOpacity;
  return float4(col, outA);
}

constant bool kGlassHasBlur [[function_constant(1)]];

static inline half4 sample_backdrop_frost_half(
    texture2d<half> backdrop_tex,
    sampler backdrop_samp,
    float2 uvCoord,
    float blurPx,
    float2 invRes,
    half glassMask) {
  if (!kGlassHasBlur || blurPx <= 0.5f) {
    return backdrop_tex.sample(backdrop_samp, uvCoord, level(0.0f));
  }
  float radiusPx = blurPx * float(glassMask);
  float maxLod = max(float(backdrop_tex.get_num_mip_levels()) - 1.0f, 0.0f);
  float lod = clamp(log2(max(radiusPx * 0.68f, 1.0f)), 0.0f, maxLod);
  float2 diagOff = (radiusPx * 0.62f) * invRes;
  float2 uvMin = float2(0.0005f);
  float2 uvMax = float2(0.9995f);

  half4 c0 = backdrop_tex.sample(backdrop_samp, uvCoord, level(lod));
  half4 d0 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(diagOff.x, diagOff.y), uvMin, uvMax), level(lod));
  half4 d1 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(-diagOff.x, diagOff.y), uvMin, uvMax), level(lod));
  half4 d2 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(diagOff.x, -diagOff.y), uvMin, uvMax), level(lod));
  half4 d3 = backdrop_tex.sample(backdrop_samp, clamp(uvCoord + float2(-diagOff.x, -diagOff.y), uvMin, uvMax), level(lod));

  return c0 * 0.36h + (d0 + d1 + d2 + d3) * 0.16h;
}

struct LiquidGlassSyncedVertexOutput {
  float4 position [[position]];
  half4 tint [[flat]];
  float2 pcenter [[flat]];
  float2 phalf [[flat]];
  float4 corner_radii [[flat]];
  float4 glass_params [[flat]];
  float4 extra_params [[flat]];
  float4 mask_inv_res [[flat]];
  float4 inner_rect [[flat]];
  float4 bevel_params [[flat]];
  half glass_opacity [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct LiquidGlassSyncedFragmentInput {
  float4 position [[position]];
  half4 tint [[flat]];
  float2 pcenter [[flat]];
  float2 phalf [[flat]];
  float4 corner_radii [[flat]];
  float4 glass_params [[flat]];
  float4 extra_params [[flat]];
  float4 mask_inv_res [[flat]];
  float4 inner_rect [[flat]];
  float4 bevel_params [[flat]];
  half glass_opacity [[flat]];
};

static inline void compute_decoupled_glass_refraction(
    float2 rel,
    float2 q,
    float d,
    float dispScale,
    float pullReach,
    float invPullReach,
    float edgeThick,
    float invEdgeThick,
    half glassMask,
    thread float2 &outDispPx,
    thread float &outBevelSheen,
    thread float &outChromaWeight) {
  float depth = -d;
  float2 dispPx = float2(0.0f);
  float bevelSheen = 0.0f;
  float chromaWeight = 0.0f;

  if (depth < edgeThick) {
    float ex;
    float ey;
    if (q.x >= q.y) {
      ex = 1.0f;
      ey = fast::exp((q.y - q.x) * invEdgeThick);
    } else {
      ex = fast::exp((q.x - q.y) * invEdgeThick);
      ey = 1.0f;
    }
    float2 edgeN = normalize(float2((rel.x >= 0.0f ? 1.0f : -1.0f) * ex,
                                    (rel.y >= 0.0f ? 1.0f : -1.0f) * ey) + float2(1e-5f));
    float xEdge = saturate(depth * invEdgeThick);
    float omx = 1.0f - xEdge;
    float omx3 = omx * omx * omx;
    float omx4 = omx3 * omx;
    float denom = max(1.0f - omx4, 1e-4f);
    float inv_s = rsqrt(denom);
    float slope = omx3 * inv_s * sqrt(inv_s);
    float inv = rsqrt(fma(slope, slope, 1.0f));
    float sinI = slope * inv;
    float sinT = sinI * 0.6666667f;
    float cosT = sqrt(max(1.0f - sinT * sinT, 0.0f));
    float edgeBend = fma(sinI, cosT, -inv * sinT);
    float edgeLip = edgeBend * (edgeThick * 0.55f);
    dispPx += -edgeN * (edgeLip * float(glassMask));
    bevelSheen = edgeBend;
    chromaWeight = max(chromaWeight, edgeBend);
  }

  if (depth < pullReach && dispScale > 0.001f) {
    float px;
    float py;
    if (q.x >= q.y) {
      px = 1.0f;
      py = fast::exp((q.y - q.x) * invPullReach);
    } else {
      px = fast::exp((q.x - q.y) * invPullReach);
      py = 1.0f;
    }
    float2 pullN = normalize(float2((rel.x >= 0.0f ? 1.0f : -1.0f) * px,
                                    (rel.y >= 0.0f ? 1.0f : -1.0f) * py) + float2(1e-5f));
    float xPull = saturate(depth * invPullReach);
    float smoothFalloff = 1.0f - smoothstep(0.0f, 1.0f, xPull);
    float pullCurve = smoothFalloff * smoothFalloff;
    dispPx += -pullN * (pullCurve * dispScale * 0.85f * float(glassMask));
    chromaWeight = max(chromaWeight, pullCurve);
  }

  outDispPx = dispPx;
  outBevelSheen = bevelSheen;
  outChromaWeight = chromaWeight;
}

vertex LiquidGlassSyncedVertexOutput liquid_glass_synced_vertex(
    uint unit_vertex_id [[vertex_id]], uint quad_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(QuadInputIndex_Vertices)]],
    constant Quad *quads [[buffer(QuadInputIndex_Quads)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(QuadInputIndex_ViewportSize)]]) {
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Quad quad = quads[quad_id];

  float4 device_position =
      to_device_position(unit_vertex, quad.bounds, viewport_size);
  float4 clip_distance = distance_from_clip_rect(unit_vertex, quad.bounds,
                                                 quad.content_mask.bounds);
  half4 tint = half4(hsla_to_rgba(quad.background.solid));

  float2 psize = float2(quad.bounds.size.width, quad.bounds.size.height);
  float2 phalf = psize * 0.5;
  float2 pcenter = float2(quad.bounds.origin.x, quad.bounds.origin.y) + phalf;
  float min_half = min(phalf.x, phalf.y);
  float4 corner_radii = float4(
      min(quad.corner_radii.top_left, min_half),
      min(quad.corner_radii.top_right, min_half),
      min(quad.corner_radii.bottom_right, min_half),
      min(quad.corner_radii.bottom_left, min_half));

  float dispScale = quad.background.gradient_angle_or_pattern_height * 2.0;
  float refractReach = quad.background.colors[0].percentage * 2.0;
  float reach = clamp(refractReach, 1.0, max(min_half, 1.0));
  float inv_reach = 1.0 / reach;
  float blurPx = quad.background.colors[1].percentage * 2.0;

  float chroma = quad.background.colors[0].color.s;
  float highlightAlpha = quad.background.colors[0].color.a;
  float edgeTintKind = quad.background.colors[1].color.s;
  float inv_psize_y_085 = 1.0 / max(psize.y * 0.85, 1.0);

  float2 vp = max(float2((float)viewport_size->width, (float)viewport_size->height), float2(1.0));
  float2 inv_res = 1.0 / vp;
  float mask_bottom_y =
      quad.content_mask.bounds.origin.y + quad.content_mask.bounds.size.height;

  bool is_decoupled = quad.background.colors[1].color.l > 1.5;
  float edge_thick = is_decoupled
      ? clamp(quad.background.colors[0].color.l * 2.0, 1.0, max(min_half, 1.0))
      : reach;
  float rim_width = max(quad.background.colors[1].color.a, 0.2);
  float4 bevel_params = float4(edge_thick, 1.0 / edge_thick, rim_width, is_decoupled ? 1.0 : 0.0);
  float active_reach = max(reach, edge_thick);

  float max_r = max(max(corner_radii.x, corner_radii.y), max(corner_radii.z, corner_radii.w));
  float safe_inset = max(active_reach, 1.5) + max_r + 1.0;
  float2 vp_guard = max(vp * 0.0005 + 1.0, float2(2.0));
  float2 origin = float2(quad.bounds.origin.x, quad.bounds.origin.y);
  float2 inner_min = max(origin + safe_inset, vp_guard);
  float2 inner_max = min(origin + psize - safe_inset, vp - vp_guard);
  float4 inner_rect = float4(inner_min.x, inner_min.y, inner_max.x, inner_max.y);

  half glass_opacity = half(saturate(1.0 - quad.background.colors[0].color.h));

  return LiquidGlassSyncedVertexOutput{
      device_position,
      tint,
      pcenter,
      phalf,
      corner_radii,
      float4(dispScale, reach, inv_reach, blurPx),
      float4(chroma, highlightAlpha, edgeTintKind, inv_psize_y_085),
      float4(quad.content_mask.bottom_fade, mask_bottom_y, inv_res.x, inv_res.y),
      inner_rect,
      bevel_params,
      glass_opacity,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

// Production branchless fragment shader for Backdrop-Synced Liquid Glass (edgeMode == 7 / 1.75).
// Eliminates per-pixel Quad buffer loads, specializes sharp vs blur via function constant,
// and short-circuits interior pixels directly to single-texel reads.
fragment half4 liquid_glass_synced_fragment(
    LiquidGlassSyncedFragmentInput input [[stage_in]],
    constant float *time_sec [[buffer(3)]],
    texture2d<half> backdrop_tex [[texture(0)]],
    sampler backdrop_samp [[sampler(0)]]) {
  float2 fragPx = input.position.xy;

  if (!kGlassHasBlur &&
      fragPx.x > input.inner_rect.x && fragPx.y > input.inner_rect.y &&
      fragPx.x < input.inner_rect.z && fragPx.y < input.inner_rect.w) {
    half4 bgSample = backdrop_tex.read(uint2(fragPx));
    half4 tint = input.tint;
    half bgVis = saturate(bgSample.a);
    half3 backdropRgb = bgVis > 0.001h ? bgSample.rgb : tint.rgb;
    half3 col = mix(backdropRgb, tint.rgb, tint.a);
    half mask_fade = 1.0h;
    float fade = input.mask_inv_res.x;
    if (fade > 0.5) {
      mask_fade = half(smoothstep(0.0, fade, input.mask_inv_res.y - fragPx.y));
    }
    half outA = mix(tint.a, 1.0h, bgVis) * mask_fade * input.glass_opacity;
    return half4(col, outA);
  }

  float2 rel = fragPx - input.pcenter;

  float r = (rel.x < 0.0)
      ? ((rel.y < 0.0) ? input.corner_radii.x : input.corner_radii.w)
      : ((rel.y < 0.0) ? input.corner_radii.y : input.corner_radii.z);

  float2 q = abs(rel) - input.phalf + r;
  float d = min(max(q.x, q.y), 0.0) + length(max(q, float2(0.0))) - r;
  half glassMask = half(1.0 - smoothstep(-1.0, 1.0, d));
  if (glassMask <= 0.001h) {
    discard_fragment();
  }

  float dispScale = input.glass_params.x;
  float reach = input.glass_params.y;
  float inv_reach = input.glass_params.z;
  float blurPx = input.glass_params.w;

  float bend = 0.0;
  float chromaWeight = 0.0;
  float2 dispPx = float2(0.0);
  if (input.bevel_params.w > 0.5) {
    compute_decoupled_glass_refraction(
        rel, q, d, dispScale, reach, inv_reach,
        input.bevel_params.x, input.bevel_params.y, glassMask,
        dispPx, bend, chromaWeight);
  } else if (-d < reach && dispScale > 0.001) {
    float ex;
    float ey;
    if (q.x >= q.y) {
      ex = 1.0;
      ey = exp((q.y - q.x) * inv_reach);
    } else {
      ex = exp((q.x - q.y) * inv_reach);
      ey = 1.0;
    }
    float2 gradN = normalize(float2((rel.x >= 0.0 ? 1.0 : -1.0) * ex,
                                    (rel.y >= 0.0 ? 1.0 : -1.0) * ey) + float2(1e-5));
    float xDepth = saturate(-d * inv_reach);
    float omx = 1.0 - xDepth;
    float omx3 = omx * omx * omx;
    float omx4 = omx3 * omx;
    float denom = max(1.0 - omx4, 1e-4);
    float inv_s = rsqrt(denom);
    float slope = omx3 * inv_s * sqrt(inv_s);
    float inv = rsqrt(fma(slope, slope, 1.0));
    float sinI = slope * inv;
    float sinT = sinI * 0.6666667;
    float cosT = sqrt(max(1.0 - sinT * sinT, 0.0));
    bend = fma(sinI, cosT, -inv * sinT);
    chromaWeight = bend;
    dispPx = -gradN * (bend * dispScale * float(glassMask));
  }

  float2 invRes = input.mask_inv_res.zw;
  float2 uvMin = float2(0.0005);
  float2 uvMax = float2(0.9995);
  float2 refractUV = clamp((fragPx + dispPx) * invRes, uvMin, uvMax);
  half4 bgSample = sample_backdrop_frost_half(backdrop_tex, backdrop_samp, refractUV, blurPx, invRes, glassMask);

  float chroma = input.extra_params.x;
  float k = chroma * 0.6;
  if (chroma > 0.001 && chromaWeight > 0.0001 && dot(dispPx, dispPx) * (k * k) > 0.0004) {
    float2 rUV = clamp((fragPx + dispPx * (1.0 - k)) * invRes, uvMin, uvMax);
    float2 bUV = clamp((fragPx + dispPx * (1.0 + k)) * invRes, uvMin, uvMax);
    if (kGlassHasBlur && blurPx > 1.5) {
      float maxLod = max(float(backdrop_tex.get_num_mip_levels()) - 1.0, 0.0);
      float cLod = clamp(log2(max(blurPx * float(glassMask) * 0.85, 1.0)), 0.0, maxLod);
      half rCh = backdrop_tex.sample(backdrop_samp, rUV, level(cLod)).r;
      half bCh = backdrop_tex.sample(backdrop_samp, bUV, level(cLod)).b;
      bgSample.rgb = half3(rCh, bgSample.g, bCh);
    } else {
      half rCh = sample_backdrop_frost_half(backdrop_tex, backdrop_samp, rUV, blurPx, invRes, glassMask).r;
      half bCh = sample_backdrop_frost_half(backdrop_tex, backdrop_samp, bUV, blurPx, invRes, glassMask).b;
      bgSample.rgb = half3(rCh, bgSample.g, bCh);
    }
  }

  half4 tint = input.tint;
  half bgVis = saturate(bgSample.a);
  half3 backdropRgb = bgVis > 0.001h ? bgSample.rgb : tint.rgb;
  half3 col = mix(backdropRgb, tint.rgb, tint.a);
  col += half3(0.72h, 0.86h, 1.0h) * (half(bend) * 0.015h * glassMask);

  half edgeGlow = 0.0h;
  float razorW = input.bevel_params.w > 0.5 ? clamp(1.25 * input.bevel_params.z, 0.4, 6.0) : 1.25;
  half razor = half((1.0 - smoothstep(0.0, razorW, -d)) * step(d, 0.0));
  if (razor > 0.001h) {
    float t = *time_sec;
    float p1 = fma(fragPx.x, 0.00185, fma(fragPx.y, 0.00105, t * -0.6139));
    float p2 = fma(fragPx.x, 0.00120, fma(fragPx.y, -0.00195, t * 0.44185));
    float wave = fma(sin(p1), 0.58, cos(p2) * 0.42);
    half waveCrest = half(smoothstep(-0.35, 0.90, wave));
    float2 waveGrad = normalize(
      float2(0.00185, 0.00105) * (cos(p1) * 0.58)
      - float2(0.00120, -0.00195) * (sin(p2) * 0.42)
      + float2(1e-5)
    );
    float2 edgeDir = normalize(rel + float2(1e-5));
    half dirRaw = half(saturate(dot(edgeDir, waveGrad) * 0.5 + 0.5));
    float topY = input.pcenter.y - input.phalf.y;
    half yNorm = half(saturate((fragPx.y - topY) * input.extra_params.w));
    half oneMinusY = 1.0h - yNorm;
    half topFade = oneMinusY * oneMinusY;

    half3 syncHue = saturate(backdropRgb * 2.8h + half3(0.10h));
    half3 tintHue = saturate(tint.rgb * 3.2h + half3(0.28h));
    half3 tintC = mix(syncHue, half3(0.92h, 0.94h, 0.98h), 0.28h);
    half specCtrl = half(saturate(input.extra_params.y));
    half amp = 0.16h + specCtrl * 0.56h;
    float edgeTintKind = input.extra_params.z;
    if (edgeTintKind > 0.75) {
      tintC = mix(syncHue, tintHue, 0.72h);
      amp = 0.28h + specCtrl * 0.68h;
    } else if (edgeTintKind > 0.25) {
      tintC = mix(syncHue, tintHue, 0.70h);
      amp = 0.28h + specCtrl * 0.68h;
    }
    half glint = topFade * 0.30h + (dirRaw * dirRaw * dirRaw) * (0.30h + 0.85h * waveCrest);
    half e = razor * (0.10h + glint * amp);
    col = col + tintC * e * glassMask;
    edgeGlow = e;
  }

  half mask_fade = 1.0h;
  float fade = input.mask_inv_res.x;
  if (fade > 0.5) {
    mask_fade = half(smoothstep(0.0, fade, input.mask_inv_res.y - fragPx.y));
  }
  half outA = mix(tint.a, 1.0h, bgVis) * glassMask;
  outA = saturate(max(outA, edgeGlow * glassMask)) * mask_fade * input.glass_opacity;
  return half4(col, outA);
}

fragment half4 liquid_glass_synced_sharp_fragment(
    LiquidGlassSyncedFragmentInput input [[stage_in]],
    constant float *time_sec [[buffer(3)]],
    texture2d<half> backdrop_tex [[texture(0)]],
    sampler backdrop_samp [[sampler(0)]],
    half4 dst [[color(0)]]) {
  float2 fragPx = input.position.xy;

  if (fragPx.x > input.inner_rect.x && fragPx.y > input.inner_rect.y &&
      fragPx.x < input.inner_rect.z && fragPx.y < input.inner_rect.w) {
    half4 tint = input.tint;
    half bgVis = saturate(dst.a);
    half3 backdropRgb = bgVis > 0.001h ? dst.rgb : tint.rgb;
    half3 col = mix(backdropRgb, tint.rgb, tint.a);
    half mask_fade = 1.0h;
    float fade = input.mask_inv_res.x;
    if (fade > 0.5) {
      mask_fade = half(smoothstep(0.0, fade, input.mask_inv_res.y - fragPx.y));
    }
    half outA = mix(tint.a, 1.0h, bgVis) * mask_fade * input.glass_opacity;
    return half4(mix(dst.rgb, col, outA), saturate(outA + dst.a));
  }

  float2 rel = fragPx - input.pcenter;
  float r = (rel.x < 0.0)
      ? ((rel.y < 0.0) ? input.corner_radii.x : input.corner_radii.w)
      : ((rel.y < 0.0) ? input.corner_radii.y : input.corner_radii.z);

  float2 q = abs(rel) - input.phalf + r;
  float d = min(max(q.x, q.y), 0.0) + length(max(q, float2(0.0))) - r;
  half glassMask = half(1.0 - smoothstep(-1.0, 1.0, d));
  if (glassMask <= 0.001h) {
    return dst;
  }

  float dispScale = input.glass_params.x;
  float reach = input.glass_params.y;
  float inv_reach = input.glass_params.z;

  float bend = 0.0;
  float2 dispPx = float2(0.0);
  if (-d < reach && dispScale > 0.001) {
    float ex;
    float ey;
    if (q.x >= q.y) {
      ex = 1.0;
      ey = fast::exp((q.y - q.x) * inv_reach);
    } else {
      ex = fast::exp((q.x - q.y) * inv_reach);
      ey = 1.0;
    }
    float2 gradN = normalize(float2((rel.x >= 0.0 ? 1.0 : -1.0) * ex,
                                    (rel.y >= 0.0 ? 1.0 : -1.0) * ey) + float2(1e-5));
    float xDepth = saturate(-d * inv_reach);
    float omx = 1.0 - xDepth;
    float omx3 = omx * omx * omx;
    float omx4 = omx3 * omx;
    float denom = max(1.0 - omx4, 1e-4);
    float inv_s = rsqrt(denom);
    float slope = omx3 * inv_s * sqrt(inv_s);
    float inv = rsqrt(fma(slope, slope, 1.0));
    float sinI = slope * inv;
    float sinT = sinI * 0.6666667;
    float cosT = sqrt(max(1.0 - sinT * sinT, 0.0));
    bend = fma(sinI, cosT, -inv * sinT);
    dispPx = -gradN * (bend * dispScale * float(glassMask));
  }

  float2 invRes = input.mask_inv_res.zw;
  float2 uvMin = float2(0.0005);
  float2 uvMax = float2(0.9995);
  float2 refractUV = clamp((fragPx + dispPx) * invRes, uvMin, uvMax);
  half4 bgSample = backdrop_tex.sample(backdrop_samp, refractUV, level(0.0));

  float chroma = input.extra_params.x;
  if (chroma > 0.001 && bend > 0.0001) {
    float k = chroma * 0.6;
    float2 rUV = clamp((fragPx + dispPx * (1.0 - k)) * invRes, uvMin, uvMax);
    float2 bUV = clamp((fragPx + dispPx * (1.0 + k)) * invRes, uvMin, uvMax);
    half rCh = backdrop_tex.sample(backdrop_samp, rUV, level(0.0)).r;
    half bCh = backdrop_tex.sample(backdrop_samp, bUV, level(0.0)).b;
    bgSample.rgb = half3(rCh, bgSample.g, bCh);
  }

  half4 tint = input.tint;
  half bgVis = saturate(bgSample.a);
  half3 backdropRgb = bgVis > 0.001h ? bgSample.rgb : tint.rgb;
  half3 col = mix(backdropRgb, tint.rgb, tint.a);
  col += half3(0.72h, 0.86h, 1.0h) * (half(bend) * 0.015h * glassMask);

  half edgeGlow = 0.0h;
  half razor = half((1.0 - smoothstep(0.0, 1.25, -d)) * step(d, 0.0));
  if (razor > 0.001h) {
    float t = *time_sec;
    float p1 = fma(fragPx.x, 0.00185, fma(fragPx.y, 0.00105, t * -0.6139));
    float p2 = fma(fragPx.x, 0.00120, fma(fragPx.y, -0.00195, t * 0.44185));
    float wave = fma(fast::sin(p1), 0.58, fast::cos(p2) * 0.42);
    half waveCrest = half(smoothstep(-0.35, 0.90, wave));
    float2 waveGrad = normalize(
      float2(0.00185, 0.00105) * (fast::cos(p1) * 0.58)
      - float2(0.00120, -0.00195) * (fast::sin(p2) * 0.42)
      + float2(1e-5)
    );
    float2 edgeDir = normalize(rel + float2(1e-5));
    half dirRaw = half(saturate(dot(edgeDir, waveGrad) * 0.5 + 0.5));
    float topY = input.pcenter.y - input.phalf.y;
    half yNorm = half(saturate((fragPx.y - topY) * input.extra_params.w));
    half oneMinusY = 1.0h - yNorm;
    half topFade = oneMinusY * oneMinusY;

    half3 syncHue = saturate(backdropRgb * 2.8h + half3(0.10h));
    half3 tintHue = saturate(tint.rgb * 3.2h + half3(0.28h));
    half3 tintC = mix(syncHue, half3(0.92h, 0.94h, 0.98h), 0.28h);
    half specCtrl = half(saturate(input.extra_params.y));
    half amp = 0.16h + specCtrl * 0.56h;
    float edgeTintKind = input.extra_params.z;
    if (edgeTintKind > 0.75) {
      tintC = mix(syncHue, tintHue, 0.72h);
      amp = 0.28h + specCtrl * 0.68h;
    } else if (edgeTintKind > 0.25) {
      tintC = mix(syncHue, tintHue, 0.70h);
      amp = 0.28h + specCtrl * 0.68h;
    }
    half glint = topFade * 0.30h + (dirRaw * dirRaw * dirRaw) * (0.30h + 0.85h * waveCrest);
    half e = razor * (0.10h + glint * amp);
    col = col + tintC * e * glassMask;
    edgeGlow = e;
  }

  half mask_fade = 1.0h;
  float fade = input.mask_inv_res.x;
  if (fade > 0.5) {
    mask_fade = half(smoothstep(0.0, fade, input.mask_inv_res.y - fragPx.y));
  }
  half outA = mix(tint.a, 1.0h, bgVis) * glassMask;
  outA = saturate(max(outA, edgeGlow * glassMask)) * mask_fade * input.glass_opacity;
  return half4(mix(dst.rgb, col, outA), saturate(outA + dst.a));
}

struct LiquidGlassInteriorVertexOutput {
  float4 position [[position]];
  half4 tint [[flat]];
  float2 mask_params [[flat]];
  half glass_opacity [[flat]];
  float clip_distance [[clip_distance]][4];
};

struct LiquidGlassInteriorFragmentInput {
  float4 position [[position]];
  half4 tint [[flat]];
  float2 mask_params [[flat]];
  half glass_opacity [[flat]];
};

static inline void compute_glass_inner_bounds(
    Quad quad,
    constant Size_DevicePixels *viewport_size,
    thread float &ox0, thread float &oy0, thread float &ox1, thread float &oy1,
    thread float &ix0, thread float &iy0, thread float &ix1, thread float &iy1) {
  float2 origin = float2(quad.bounds.origin.x, quad.bounds.origin.y);
  float2 psize = float2(quad.bounds.size.width, quad.bounds.size.height);
  float2 phalf = psize * 0.5f;
  float min_half = min(phalf.x, phalf.y);
  float max_r = max(
      max(min(quad.corner_radii.top_left, min_half), min(quad.corner_radii.top_right, min_half)),
      max(min(quad.corner_radii.bottom_right, min_half), min(quad.corner_radii.bottom_left, min_half)));
  float pull_reach = clamp(quad.background.colors[0].percentage * 2.0f, 1.0f, max(min_half, 1.0f));
  float reach = quad.background.colors[1].color.l > 1.5f
      ? max(pull_reach, clamp(quad.background.colors[0].color.l * 2.0f, 1.0f, max(min_half, 1.0f)))
      : pull_reach;
  float reach_eff = max(reach, 2.0f);
  float safe_inset = max(reach_eff, 0.29289322f * max_r + 0.70710678f * reach_eff) + 1.5f;
  float2 vp = max(float2((float)viewport_size->width, (float)viewport_size->height), float2(1.0f));
  float2 vp_guard = max(vp * 0.0005f + 1.0f, float2(2.0f));
  float2 inner_min = max(origin + safe_inset, vp_guard);
  float2 inner_max = min(origin + psize - safe_inset, vp - vp_guard);

  ox0 = floor(origin.x);
  oy0 = floor(origin.y);
  ox1 = ceil(origin.x + psize.x);
  oy1 = ceil(origin.y + psize.y);
  ix0 = clamp(ceil(inner_min.x), ox0, ox1);
  iy0 = clamp(ceil(inner_min.y), oy0, oy1);
  ix1 = clamp(floor(inner_max.x), ix0, ox1);
  iy1 = clamp(floor(inner_max.y), iy0, oy1);
}

vertex LiquidGlassInteriorVertexOutput liquid_glass_synced_interior_vertex(
    uint unit_vertex_id [[vertex_id]], uint quad_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(QuadInputIndex_Vertices)]],
    constant Quad *quads [[buffer(QuadInputIndex_Quads)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(QuadInputIndex_ViewportSize)]]) {
  Quad quad = quads[quad_id];
  half4 tint = half4(hsla_to_rgba(quad.background.solid));
  float mask_bottom_y =
      quad.content_mask.bounds.origin.y + quad.content_mask.bounds.size.height;

  half glass_opacity = half(saturate(1.0f - quad.background.colors[0].color.h));

  float ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1;
  compute_glass_inner_bounds(quad, viewport_size, ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1);
  if (ix1 <= ix0 || iy1 <= iy0) {
    return LiquidGlassInteriorVertexOutput{
        float4(2.0f, 2.0f, 0.0f, 1.0f),
        tint,
        float2(quad.content_mask.bottom_fade, mask_bottom_y),
        glass_opacity,
        {-1.0f, -1.0f, -1.0f, -1.0f}};
  }
  float2 unit_vertex = unit_vertices[unit_vertex_id];
  Bounds_ScaledPixels bounds = make_sub_bounds(ix0, iy0, ix1, iy1);
  float4 device_position = to_device_position(unit_vertex, bounds, viewport_size);
  float4 clip_distance =
      distance_from_clip_rect(unit_vertex, bounds, quad.content_mask.bounds);
  return LiquidGlassInteriorVertexOutput{
      device_position,
      tint,
      float2(quad.content_mask.bottom_fade, mask_bottom_y),
      glass_opacity,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment half4 liquid_glass_synced_interior_fragment(
    LiquidGlassInteriorFragmentInput input [[stage_in]],
    half4 dst [[color(0)]]) {
  half4 tint = input.tint;
  half bgVis = saturate(dst.a);
  half3 backdropRgb = bgVis > 0.001h ? dst.rgb : tint.rgb;
  half3 col = mix(backdropRgb, tint.rgb, tint.a);
  half mask_fade = 1.0h;
  float fade = input.mask_params.x;
  if (fade > 0.5f) {
    mask_fade = half(smoothstep(0.0f, fade, input.mask_params.y - input.position.y));
  }
  half outA = mix(tint.a, 1.0h, bgVis) * mask_fade * input.glass_opacity;
  return half4(mix(dst.rgb, col, outA), saturate(outA + dst.a));
}

vertex LiquidGlassSyncedVertexOutput liquid_glass_synced_rim_vertex(
    uint vertex_id [[vertex_id]], uint quad_id [[instance_id]],
    constant float2 *unit_vertices [[buffer(QuadInputIndex_Vertices)]],
    constant Quad *quads [[buffer(QuadInputIndex_Quads)]],
    constant Size_DevicePixels *viewport_size
    [[buffer(QuadInputIndex_ViewportSize)]]) {
  Quad quad = quads[quad_id];
  half4 tint = half4(hsla_to_rgba(quad.background.solid));

  float2 psize = float2(quad.bounds.size.width, quad.bounds.size.height);
  float2 phalf = psize * 0.5f;
  float2 pcenter = float2(quad.bounds.origin.x, quad.bounds.origin.y) + phalf;
  float min_half = min(phalf.x, phalf.y);
  float4 corner_radii = float4(
      min(quad.corner_radii.top_left, min_half),
      min(quad.corner_radii.top_right, min_half),
      min(quad.corner_radii.bottom_right, min_half),
      min(quad.corner_radii.bottom_left, min_half));

  float dispScale = quad.background.gradient_angle_or_pattern_height * 2.0f;
  float refractReach = quad.background.colors[0].percentage * 2.0f;
  float reach = clamp(refractReach, 1.0f, max(min_half, 1.0f));
  float inv_reach = 1.0f / reach;
  float blurPx = quad.background.colors[1].percentage * 2.0f;

  float chroma = quad.background.colors[0].color.s;
  float highlightAlpha = quad.background.colors[0].color.a;
  float edgeTintKind = quad.background.colors[1].color.s;
  float inv_psize_y_085 = 1.0f / max(psize.y * 0.85f, 1.0f);

  float2 vp = max(float2((float)viewport_size->width, (float)viewport_size->height), float2(1.0f));
  float2 inv_res = 1.0f / vp;
  float mask_bottom_y =
      quad.content_mask.bounds.origin.y + quad.content_mask.bounds.size.height;

  float ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1;
  compute_glass_inner_bounds(quad, viewport_size, ox0, oy0, ox1, oy1, ix0, iy0, ix1, iy1);

  uint sub = vertex_id / 6u;
  float rx0, ry0, rx1, ry1;
  bool has_hole = (ix1 > ix0) && (iy1 > iy0);
  if (!has_hole) {
    if (sub == 0u) {
      rx0 = ox0; ry0 = oy0; rx1 = ox1; ry1 = oy1;
    } else {
      rx0 = ox0; ry0 = oy0; rx1 = ox0; ry1 = oy0;
    }
  } else if (sub == 0u) {
    rx0 = ox0; ry0 = oy0; rx1 = ox1; ry1 = iy0;
  } else if (sub == 1u) {
    rx0 = ox0; ry0 = iy1; rx1 = ox1; ry1 = oy1;
  } else if (sub == 2u) {
    rx0 = ox0; ry0 = iy0; rx1 = ix0; ry1 = iy1;
  } else {
    rx0 = ix1; ry0 = iy0; rx1 = ox1; ry1 = iy1;
  }

  bool is_decoupled = quad.background.colors[1].color.l > 1.5f;
  float edge_thick = is_decoupled
      ? clamp(quad.background.colors[0].color.l * 2.0f, 1.0f, max(min_half, 1.0f))
      : reach;
  float rim_width = max(quad.background.colors[1].color.a, 0.2f);
  float4 bevel_params = float4(edge_thick, 1.0f / edge_thick, rim_width, is_decoupled ? 1.0f : 0.0f);

  half glass_opacity = half(saturate(1.0f - quad.background.colors[0].color.h));

  if (rx1 <= rx0 || ry1 <= ry0) {
    return LiquidGlassSyncedVertexOutput{
        float4(2.0f, 2.0f, 0.0f, 1.0f),
        tint,
        pcenter,
        phalf,
        corner_radii,
        float4(dispScale, reach, inv_reach, blurPx),
        float4(chroma, highlightAlpha, edgeTintKind, inv_psize_y_085),
        float4(quad.content_mask.bottom_fade, mask_bottom_y, inv_res.x, inv_res.y),
        float4(0.0f),
        bevel_params,
        glass_opacity,
        {-1.0f, -1.0f, -1.0f, -1.0f}};
  }

  float2 unit_vertex = unit_vertices[vertex_id % 6u];
  Bounds_ScaledPixels bounds = make_sub_bounds(rx0, ry0, rx1, ry1);
  float4 device_position = to_device_position(unit_vertex, bounds, viewport_size);
  float4 clip_distance =
      distance_from_clip_rect(unit_vertex, bounds, quad.content_mask.bounds);
  return LiquidGlassSyncedVertexOutput{
      device_position,
      tint,
      pcenter,
      phalf,
      corner_radii,
      float4(dispScale, reach, inv_reach, blurPx),
      float4(chroma, highlightAlpha, edgeTintKind, inv_psize_y_085),
      float4(quad.content_mask.bottom_fade, mask_bottom_y, inv_res.x, inv_res.y),
      float4(0.0f),
      bevel_params,
      glass_opacity,
      {clip_distance.x, clip_distance.y, clip_distance.z, clip_distance.w}};
}

fragment half4 liquid_glass_synced_rim_fragment(
    LiquidGlassSyncedFragmentInput input [[stage_in]],
    constant float *time_sec [[buffer(3)]],
    texture2d<half> backdrop_tex [[texture(0)]],
    sampler backdrop_samp [[sampler(0)]]) {
  float2 fragPx = input.position.xy;
  float2 rel = fragPx - input.pcenter;
  float r = (rel.x < 0.0f)
      ? ((rel.y < 0.0f) ? input.corner_radii.x : input.corner_radii.w)
      : ((rel.y < 0.0f) ? input.corner_radii.y : input.corner_radii.z);

  float2 q = abs(rel) - input.phalf + r;
  float d = min(max(q.x, q.y), 0.0f) + length(max(q, float2(0.0f))) - r;
  half glassMask = half(1.0f - smoothstep(-1.0f, 1.0f, d));
  if (glassMask <= 0.001h) {
    discard_fragment();
  }

  float dispScale = input.glass_params.x;
  float reach = input.glass_params.y;
  float inv_reach = input.glass_params.z;

  float bend = 0.0f;
  float chromaWeight = 0.0f;
  float2 dispPx = float2(0.0f);
  if (input.bevel_params.w > 0.5f) {
    compute_decoupled_glass_refraction(
        rel, q, d, dispScale, reach, inv_reach,
        input.bevel_params.x, input.bevel_params.y, glassMask,
        dispPx, bend, chromaWeight);
  } else if (-d < reach && dispScale > 0.001f) {
    float ex;
    float ey;
    if (q.x >= q.y) {
      ex = 1.0f;
      ey = fast::exp((q.y - q.x) * inv_reach);
    } else {
      ex = fast::exp((q.x - q.y) * inv_reach);
      ey = 1.0f;
    }
    float2 gradN = normalize(float2((rel.x >= 0.0f ? 1.0f : -1.0f) * ex,
                                    (rel.y >= 0.0f ? 1.0f : -1.0f) * ey) + float2(1e-5f));
    float xDepth = saturate(-d * inv_reach);
    float omx = 1.0f - xDepth;
    float omx3 = omx * omx * omx;
    float omx4 = omx3 * omx;
    float denom = max(1.0f - omx4, 1e-4f);
    float inv_s = rsqrt(denom);
    float slope = omx3 * inv_s * sqrt(inv_s);
    float inv = rsqrt(fma(slope, slope, 1.0f));
    float sinI = slope * inv;
    float sinT = sinI * 0.6666667f;
    float cosT = sqrt(max(1.0f - sinT * sinT, 0.0f));
    bend = fma(sinI, cosT, -inv * sinT);
    chromaWeight = bend;
    dispPx = -gradN * (bend * dispScale * float(glassMask));
  }

  float2 invRes = input.mask_inv_res.zw;
  float2 uvMin = float2(0.0005f);
  float2 uvMax = float2(0.9995f);
  float2 refractUV = clamp((fragPx + dispPx) * invRes, uvMin, uvMax);
  half4 bgSample = backdrop_tex.sample(backdrop_samp, refractUV, level(0.0));

  float chroma = input.extra_params.x;
  float k = chroma * 0.6f;
  if (chroma > 0.001f && chromaWeight > 0.0001f && dot(dispPx, dispPx) * (k * k) > 0.0004f) {
    float2 rUV = clamp((fragPx + dispPx * (1.0f - k)) * invRes, uvMin, uvMax);
    float2 bUV = clamp((fragPx + dispPx * (1.0f + k)) * invRes, uvMin, uvMax);
    half rCh = backdrop_tex.sample(backdrop_samp, rUV, level(0.0f)).r;
    half bCh = backdrop_tex.sample(backdrop_samp, bUV, level(0.0f)).b;
    bgSample.rgb = half3(rCh, bgSample.g, bCh);
  }

  half4 tint = input.tint;
  half bgVis = saturate(bgSample.a);
  half3 backdropRgb = bgVis > 0.001h ? bgSample.rgb : tint.rgb;
  half3 col = mix(backdropRgb, tint.rgb, tint.a);
  col += half3(0.72h, 0.86h, 1.0h) * (half(bend) * 0.015h * glassMask);

  half edgeGlow = 0.0h;
  float razorW = input.bevel_params.w > 0.5f ? clamp(1.25f * input.bevel_params.z, 0.4f, 6.0f) : 1.25f;
  half razor = half((1.0f - smoothstep(0.0f, razorW, -d)) * step(d, 0.0f));
  if (razor > 0.001h) {
    float t = *time_sec;
    float p1 = fma(fragPx.x, 0.00185f, fma(fragPx.y, 0.00105f, t * -0.6139f));
    float p2 = fma(fragPx.x, 0.00120f, fma(fragPx.y, -0.00195f, t * 0.44185f));
    float wave = fma(fast::sin(p1), 0.58f, fast::cos(p2) * 0.42f);
    half waveCrest = half(smoothstep(-0.35f, 0.90f, wave));
    float2 waveGrad = normalize(
      float2(0.00185f, 0.00105f) * (fast::cos(p1) * 0.58f)
      - float2(0.00120f, -0.00195f) * (fast::sin(p2) * 0.42f)
      + float2(1e-5f)
    );
    float2 edgeDir = normalize(rel + float2(1e-5f));
    half dirRaw = half(saturate(dot(edgeDir, waveGrad) * 0.5f + 0.5f));
    float topY = input.pcenter.y - input.phalf.y;
    half yNorm = half(saturate((fragPx.y - topY) * input.extra_params.w));
    half oneMinusY = 1.0h - yNorm;
    half topFade = oneMinusY * oneMinusY;

    half3 syncHue = saturate(backdropRgb * 2.8h + half3(0.10h));
    half3 tintHue = saturate(tint.rgb * 3.2h + half3(0.28h));
    half3 tintC = mix(syncHue, half3(0.92h, 0.94h, 0.98h), 0.28h);
    half specCtrl = half(saturate(input.extra_params.y));
    half amp = 0.16h + specCtrl * 0.56h;
    float edgeTintKind = input.extra_params.z;
    if (edgeTintKind > 0.75f) {
      tintC = mix(syncHue, tintHue, 0.72h);
      amp = 0.28h + specCtrl * 0.68h;
    } else if (edgeTintKind > 0.25f) {
      tintC = mix(syncHue, tintHue, 0.70h);
      amp = 0.28h + specCtrl * 0.68h;
    }
    half glint = topFade * 0.30h + (dirRaw * dirRaw * dirRaw) * (0.30h + 0.85h * waveCrest);
    half e = razor * (0.10h + glint * amp);
    col = col + tintC * e * glassMask;
    edgeGlow = e;
  }

  half mask_fade = 1.0h;
  float fade = input.mask_inv_res.x;
  if (fade > 0.5f) {
    mask_fade = half(smoothstep(0.0f, fade, input.mask_inv_res.y - fragPx.y));
  }
  half outA = mix(tint.a, 1.0h, bgVis) * glassMask;
  outA = saturate(max(outA, edgeGlow * glassMask)) * mask_fade * input.glass_opacity;
  return half4(col, outA);
}

struct MipRectUniforms {
  uint dst_ox;
  uint dst_oy;
  uint dst_w;
  uint dst_h;
  float inv_src_w;
  float inv_src_h;
};

kernel void backdrop_downsample_rect_kernel(
    texture2d<half, access::sample> src_tex [[texture(0)]],
    texture2d<half, access::write> dst_tex [[texture(1)]],
    sampler linear_samp [[sampler(0)]],
    constant MipRectUniforms &u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]) {
  if (gid.x >= u.dst_w || gid.y >= u.dst_h) {
    return;
  }
  uint dx = u.dst_ox + gid.x;
  uint dy = u.dst_oy + gid.y;
  float2 uv = float2(float(dx * 2u + 1u) * u.inv_src_w, float(dy * 2u + 1u) * u.inv_src_h);
  float2 off = float2(u.inv_src_w, u.inv_src_h);
  half4 c = src_tex.sample(linear_samp, uv, level(0.0f)) * 0.5h
          + src_tex.sample(linear_samp, uv + float2(-off.x, -off.y), level(0.0f)) * 0.125h
          + src_tex.sample(linear_samp, uv + float2(off.x, -off.y), level(0.0f)) * 0.125h
          + src_tex.sample(linear_samp, uv + float2(-off.x, off.y), level(0.0f)) * 0.125h
          + src_tex.sample(linear_samp, uv + float2(off.x, off.y), level(0.0f)) * 0.125h;
  dst_tex.write(c, uint2(dx, dy));
}
