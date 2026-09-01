// Page Curl Shader for Buffer Transitions
// Simulates a 3D page turn effect like flipping a book page
//
// Based on cylinder-based page curl algorithm

uniform float u_progress;      // 0.0 = flat, 1.0 = fully turned
uniform float u_radius;        // curl cylinder radius
uniform vec2 u_resolution;     // page size (width, height)
uniform float u_shadow;        // shadow intensity
uniform float u_backside_darken; // how much to darken backside

uniform sampler2D u_old_page;  // texture of old buffer
uniform sampler2D u_new_page;  // texture of new buffer

// The curl line moves from right to left as progress increases
// Everything to the right of curl line shows old page curling
// Everything to the left shows new page

void mainImage(out vec4 fragColor, in vec2 fragCoord, in vec2 resolution, out vec2 texCoord) {
    vec2 uv = fragCoord / resolution;

    // Curl line position (moves from right edge to left as progress increases)
    float curl_x = 1.0 - u_progress * 1.5; // Goes past left edge

    // Cylinder axis is vertical, positioned at curl_x
    float dist_to_curl = uv.x - curl_x;

    if (dist_to_curl < 0.0) {
        // Left of curl line - show new page
        texCoord = uv;
        vec4 new_color = GskTexture(u_new_page, uv);

        // Add shadow near the curl line
        float shadow_dist = -dist_to_curl;
        float shadow = 1.0 - u_shadow * exp(-shadow_dist * 10.0) * u_progress;

        fragColor = new_color * shadow;
    } else {
        // Right of curl line - this part is curling
        float angle = dist_to_curl / u_radius; // Angle around cylinder

        if (angle > 3.14159) {
            // Past the back of the cylinder - show new page (or transparent)
            fragColor = vec4(0.0);
            texCoord = uv;
        } else if (angle > 1.5708) {
            // On the back of the page (visible as it curls over)
            // Map back to original texture coordinates
            float back_x = curl_x + (3.14159 - angle) * u_radius;
            vec2 back_uv = vec2(back_x, uv.y);

            if (back_uv.x >= 0.0 && back_uv.x <= 1.0) {
                texCoord = back_uv;
                vec4 back_color = GskTexture(u_old_page, back_uv);
                // Darken backside and flip horizontally
                back_color.rgb *= (1.0 - u_backside_darken);
                fragColor = back_color;
            } else {
                fragColor = vec4(0.0);
                texCoord = uv;
            }
        } else {
            // On the front of the curling page
            float front_x = curl_x + angle * u_radius;
            vec2 front_uv = vec2(front_x, uv.y);

            if (front_uv.x >= 0.0 && front_uv.x <= 1.0) {
                texCoord = front_uv;
                vec4 front_color = GskTexture(u_old_page, front_uv);

                // Add lighting based on angle (brighten as it faces the light)
                float light = 0.8 + 0.2 * cos(angle);
                front_color.rgb *= light;

                fragColor = front_color;
            } else {
                fragColor = vec4(0.0);
                texCoord = uv;
            }
        }
    }
}
