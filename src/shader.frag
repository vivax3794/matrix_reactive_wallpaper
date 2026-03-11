float hash(float n) {
    return fract(sin(n) * 43758.5453123);
}

float charGlyph(vec2 uv, float seed) {
    vec2 g = floor(uv * vec2(3.0, 5.0));
    if (g.x < 0.0 || g.x > 2.0 || g.y < 0.0 || g.y > 4.0) return 0.0;
    float idx = g.y * 3.0 + g.x;
    float structural = 0.0;
    float s = floor(seed);
    float vertL  = step(0.5, hash(s * 13.1));
    float vertR  = step(0.5, hash(s * 27.3));
    float horizT = step(0.5, hash(s * 41.7));
    float horizM = step(0.5, hash(s * 59.1));
    float horizB = step(0.5, hash(s * 73.3));
    float diagF  = step(0.6, hash(s * 89.7));
    float crossV = step(0.5, hash(s * 103.1));
    if (g.x == 0.0 && vertL > 0.5) structural = 1.0;
    if (g.x == 2.0 && vertR > 0.5) structural = 1.0;
    if (g.y == 0.0 && horizT > 0.5) structural = 1.0;
    if (g.y == 2.0 && horizM > 0.5) structural = 1.0;
    if (g.y == 4.0 && horizB > 0.5) structural = 1.0;
    if (diagF > 0.5 && abs(g.x - 1.0) < 1.5 && abs(g.y - g.x * 4.0 / 2.0) < 1.0) structural = 1.0;
    if (g.x == 1.0 && crossV > 0.5) structural = 1.0;
    float noise = step(0.7, hash(s * 127.1 + idx * 31.7));
    return max(structural, noise);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    float mem  = u_mem;
    float temp = u_temp;

    float cellW = 11.0;
    float cellH = 18.0;
    float numCols = floor(iResolution.x / cellW);
    float numRows = floor(iResolution.y / cellH);

    vec2 cell = floor(fragCoord / vec2(cellW, cellH));
    vec2 cellUV = fract(fragCoord / vec2(cellW, cellH));

    cell.y = numRows - 1.0 - cell.y;
    float col = cell.x;
    float row = cell.y;
    float rowCont = numRows - fragCoord.y / cellH;

    float ch1 = hash(col * 127.1 + 0.7);
    float ch2 = hash(col * 311.7 + 0.3);
    float ch3 = hash(col * 73.13 + 0.9);
    float ch4 = hash(col * 419.3 + 0.1);

    float isActive = step(ch1, mem * 1.15);
    if (isActive < 0.5) {
        fragColor = vec4(0.0, 0.0, 0.0, 0.0);
        return;
    }

    float baseSpeed = mix(2.5, 4.5, ch2);

    float trailLen = mix(5.0, 25.0, ch3);
    float cycleLen = numRows + trailLen + mix(5.0, 20.0, ch4);
    int coreIdx = int(col * float(u_num_cores) / numCols);
    coreIdx = clamp(coreIdx, 0, u_num_cores - 1);
    float headPos = mod(u_core_phases[coreIdx] * baseSpeed + ch1 * cycleLen, cycleLen);

    float dist = headPos - rowCont;

    float brightness = 0.0;
    if (dist > 0.0 && dist < trailLen) {
        float t = dist / trailLen;
        brightness = pow(1.0 - t, 2.2);
        brightness *= 0.85;
    }

    float headGlow = smoothstep(1.8, 0.0, abs(dist - 0.3));
    brightness = max(brightness, headGlow);

    if (brightness < 0.005) {
        fragColor = vec4(0.0, 0.0, 0.0, 0.0);
        return;
    }

    float charSeed;
    if (dist > -0.5 && dist < 2.0) {
        charSeed = hash(col * 127.1 + row * 311.7 + floor(iTime * 14.0) * 71.3);
    } else {
        charSeed = hash(col * 127.1 + row * 311.7 + floor(iTime * 0.3 + col * 0.1) * 71.3);
    }

    vec2 charUV = (cellUV - 0.15) / 0.7;
    float charPixel = 0.0;
    if (charUV.x >= 0.0 && charUV.x <= 1.0 && charUV.y >= 0.0 && charUV.y <= 1.0) {
        charPixel = charGlyph(vec2(charUV.x, 1.0 - charUV.y), floor(charSeed * 64.0));
    }

    vec3 coldColor  = vec3(0.49, 0.81, 1.0);   // tokyonight cyan  #7dcfff
    vec3 coolColor  = vec3(0.48, 0.64, 0.97);  // tokyonight blue  #7aa2f7
    vec3 warmColor  = vec3(0.73, 0.60, 0.97);  // tokyonight magenta #bb9af7
    vec3 hotColor   = vec3(0.97, 0.46, 0.56);  // tokyonight red   #f7768e

    vec3 color;
    if (temp < 0.33) {
        color = mix(coldColor, coolColor, temp / 0.33);
    } else if (temp < 0.66) {
        color = mix(coolColor, warmColor, (temp - 0.33) / 0.33);
    } else {
        color = mix(warmColor, hotColor, (temp - 0.66) / 0.34);
    }

    vec3 headHighlight = mix(color, vec3(0.75, 0.79, 0.96), 0.75); // tokyonight fg #c0caf5
    vec3 finalColor = mix(color, headHighlight, headGlow);

    vec3 bg = vec3(0.1, 0.11, 0.15); // tokyonight bg #1a1b26
    float lit = brightness * charPixel;
    float ambientGlow = brightness * 0.12;
    float bgAlpha = brightness * 0.6;

    vec3 outColor = mix(bg, finalColor, lit + ambientGlow);
    float alpha = clamp(bgAlpha + lit + ambientGlow, 0.0, 1.0);
    fragColor = vec4(outColor * alpha, alpha);
}
