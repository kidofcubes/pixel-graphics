#define FLT_MAX 3.402823466e+38
#define FLT_MIN 1.175494351e-38

vec4 getPixel(in ivec2 pos){
    vec4 pixel = texelFetch(iChannel1, pos, 0);
    //pixel.w = length(pixel.xyz) > 1.2 ? 1.0 : 0.0;
    //pixel.w = step(0.9,pixel.w);
    return pixel;
}
float rand(int diff){
    return texelFetch(iChannel3, ivec2((iFrame+diff)%64,((iFrame+diff)/64)%64), 0).x;
}

bool lineOfSight(in ivec2 pos, in ivec2 dest){
    if(pos==dest){
        return true;
    }

    // -1 to compensate for 1-indexed tile coordinates
    // local tileOffsetX = (ray.dirX > 0 and 1 or 0) - 1
    // local tileOffsetY = (ray.dirY > 0 and 1 or 0) - 1

    // local curX, curY = ray.startX, ray.startY
    vec2 initPos = vec2(pos);
    //initPos.x += rand(0);
    //initPos.y += rand(1);
    initPos.x += rand(0);
    initPos.y += rand(1);
    vec2 destPos = vec2(dest);
    destPos.x += rand(2);
    destPos.y += rand(3);


    vec2 dir = normalize(vec2(destPos-initPos));

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

    int taxicabDistance = abs(dest.x-pos.x)+abs(dest.y-pos.y);
    ivec2 movingPos = ivec2(int(pos.x),int(pos.y));
    if(getPixel(movingPos).z > 0.5){
        return false;
    }
    for(int i=0;i<taxicabDistance-1;i++) {
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

        curPos = vec2(initPos.x + (dir.x*t),initPos.y + (dir.y*t));
        if(getPixel(movingPos).z > 0.5){
            return false;
        }
    }
    return true;
}

//i generate the random dir
vec2 reversePathTracing(in ivec2 pos, in float seed){

    // local curX, curY = ray.startX, ray.startY
    float initSeed = noise2(float(iFrame),seed);
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
            return vec2(getPixel(pos).y,dist);
        }

        dist = length(curPos-initPos);
        curPos = vec2(initPos.x + (dir.x*t),initPos.y + (dir.y*t));
        if(getPixel(movingPos).z > 0.5){
            return vec2(getPixel(movingPos).y+getPixel(pos).y,dist);
        }
    }
    return vec2(getPixel(pos).y,LIGHT_MAX_RADIUS+1.0);
}


void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    //vec2 uv = fragCoord/iResolution.xy;
    //vec4 prev = texture(iChannel0, uv);
    //vec2 uv = fragCoord/iResolution.xy;

    ivec2 pos = ivec2(fragCoord);
    vec4 prev = texelFetch(iChannel0, pos, 0);
    if(pos.x >= RESOLUTIONX || pos.y >= RESOLUTIONY){
        fragColor = vec4(0.0,0.0,0.0,0.0);
        return;
    }
    //ivec2 mousePos = ivec2(iMouse.xy/2.0);


    float blend = 1.0 / float(KEEP_FRAMES);

    // accumulate
    vec4 newResult = vec4(0.0);

    vec4 scene_data = texelFetch(iChannel1, pos, 0);
    vec4 unpackedColors = UNPACK4U(scene_data.x,vec2(0,1.0));
    int n=25;
    for(int i=0;i<n;i++){
        //bool canReach = lineOfSight(mousePos,pos);
        vec4 result = unpackedColors;
        result.w=1.0;

        if(scene_data.z>0.5){
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

    fragColor = accum;
}