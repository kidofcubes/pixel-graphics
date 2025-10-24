in vec2 texcoords;
layout(location = 0) out vec4 pixelColor;
uniform sampler2D fb0tex;
uniform usampler2D scene;
uniform int frame;

#define FLT_MAX 3.402823466e+38
#define FLT_MIN 1.175494351e-38

//i generate the random dir
vec2 reversePathTracing(in ivec2 pos, in float seed){

    // local curX, curY = ray.startX, ray.startY
//    float initSeed = noise2(float(1.0),seed);
    float initSeed = noise2(float(frame),seed);
    vec2 initPos = vec2(pos);
    initPos.x += noise3(initSeed,initPos.x,initPos.y);
    initPos.y += noise3(initSeed,initPos.x,initPos.y);

    float randomDir = noise3(initSeed,initPos.x,initPos.y)*2.0*PI;
    vec2 dir = vec2(cos(randomDir),sin(randomDir));

    // -1 to compensate for 1-indexed tile coordinates
    int tileOffsetX = (dir.x > 0.0 ? 1 : 0);
    int tileOffsetY = (dir.y > 0.0 ? 1 : 0);

    int stepX = dir.x > 0.0 ? 1 : -1;
    int stepY = dir.y > 0.0 ? 1 : -1;

    vec2 curPos = initPos;
    // local tileX, tileY = tileCoords(grid.cellSize, curX, curY)
    float t = 0.0;
    //float tDeltaX = float(stepX)/dir.x;
    //float tDeltaY = float(stepY)/dir.y;

    if(dir.x==0.0){
        dir.x = FLT_MIN;
    }
    if(dir.y==0.0){
        dir.y = FLT_MIN;
    }

    //int taxicabDistance = abs(dest.x-pos.x)+abs(dest.y-pos.y);
    ivec2 movingPos = ivec2(int(pos.x),int(pos.y));
    //if(getPixel(movingPos).z > 0.5){
    //    return vec2(getPixel(movingPos).y,0.0);
    //}
    float dist = length(curPos-initPos);
    uvec4 currentPixel = texelFetch(scene, movingPos, 0);
    for(int i=0;i<(int(LIGHT_MAX_RADIUS*2.0));i++) {
        // while tileX > 0 and tileX <= grid.width and tileY > 0 and tileY <= grid.height do
        // grid[tileY][tileX] = true
        // mark(curX, curY)
        float dtX = (float(movingPos.x+tileOffsetX) - curPos.x) / dir.x; // distances to next borders
        float dtY = (float(movingPos.y+tileOffsetY) - curPos.y) / dir.y; // distances to next borders


        if(dtX < dtY){
            t = t + dtX;
            movingPos.x+=stepX;
        }else{
            t = t + dtY;
            movingPos.y+=stepY;
        }

        if(movingPos.x < 0 || movingPos.x >= RESOLUTIONX || movingPos.y < 0|| movingPos.y >= RESOLUTIONY || dist > LIGHT_MAX_RADIUS){
            return vec2(uintBitsToFloat(texelFetch(scene, pos, 0).y),dist);
        }

        dist = length(curPos-initPos);
        curPos = vec2(initPos.x + (dir.x*t),initPos.y + (dir.y*t));
        currentPixel = texelFetch(scene, movingPos, 0);
        if(currentPixel.z > 0.5){
            return vec2(uintBitsToFloat(currentPixel).y+uintBitsToFloat(texelFetch(scene,pos, 0).y),dist);
        }
    }
    return vec2(uintBitsToFloat(texelFetch(scene, pos, 0).y),LIGHT_MAX_RADIUS+1.0);
}


//void mainImage( out vec4 fragColor, in vec2 fragCoord )
void main()
{
    //vec2 uv = fragCoord/iResolution.xy;
    //vec4 prev = texture(iChannel0, uv);
    //vec2 uv = fragCoord/iResolution.xy;

    ivec2 pos = ivec2(gl_FragCoord);
    vec4 prev = texelFetch(fb0tex, pos, 0);
//    if(pos.x >= RESOLUTIONX || pos.y >= RESOLUTIONY){
//        pixelColor = vec4(0.0,0.0,0.0,0.0);
//        return;
//    }
    //ivec2 mousePos = ivec2(iMouse.xy/2.0);


    float blend = 1.0 / float(KEEP_FRAMES);

    // accumulate
    vec4 newResult = vec4(0.0);

    uvec4 scene_data = texelFetch(scene, pos, 0);
//    vec4 unpackedColors = UNPACK4U(scene_data.x,vec2(0,1.0));
    vec4 unpackedColors = unpackUnorm4x8(scene_data.x);
    int n=1;
    for(int i=0;i<n;i++){
        //bool canReach = lineOfSight(mousePos,pos);
        vec4 result = unpackedColors;
        result.w=1.0;

        if(uintBitsToFloat(scene_data.z)>0.5){
            //result.w=1.0;
        }else{
            //result.w=0.1;
            result.xyz *= 0.1;
        }
        vec2 raytraceResult = reversePathTracing(pos,float(i));
        //result.xyz *= raytraceResult.x * lightIntensity(raytraceResult.y);
        result.xyz *= raytraceResult.x;

        //if(canReach){
        //    result.xyz *= 1.0;
        //}else{
        //    result.xyz *= 0.0;
        //}

        newResult += result;
    }
    newResult/=float(n);
    vec4 accum = mix(prev, newResult, blend);

    pixelColor = accum;
//    pixelColor = unpackedColors;
}