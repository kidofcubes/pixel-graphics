//X holds 4 0-255 ints, RGBA
//Y holds emissiveness
//Z holds opacity (opaque or transparent, >0.5 is opaque)
void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    ivec2 pos = ivec2(fragCoord);
    vec4 color = texelFetch(iChannel0, pos, 0);

    float emissiveness = 0.0;

    float opacity = color.w;
    if(color.w<0.99){
        opacity = 0.0;
    }else{
        opacity = 1.0;
        if(length(color.xyz) > 1.4){
            emissiveness = 100.0;
        }else{
            color.xyz = vec3(1.0,0.0,1.0);
        }

    }
    ivec2 mousePos = ivec2((iMouse.xy/iResolution.xy)*vec2(RESOLUTIONX,RESOLUTIONY));
//    if(getKeyState(87,2)){
//        if(length(vec2(mousePos-pos))<3.0){
//            opacity=1.0;
//        }
//    }else{
//        if(length(vec2(mousePos-pos))<1.0){
//            emissiveness = 100.0;
//            opacity=1.0;
//        }
//    }
    //opacity=1.0;

    float packedColor = PACK4U(color,vec2(0.0,1.0));

    fragColor = vec4(packedColor,emissiveness,opacity,1.0);
}