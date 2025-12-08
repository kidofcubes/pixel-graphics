//X holds 4 0-255 ints, RGBA
//Y holds emissiveness
//Z holds opacity (opaque or transparent, >0.5 is opaque)
void mainImage( out vec4 fragColor, in vec2 fragCoord )
{
    ivec2 pos = ivec2(fragCoord);
    vec4 color = texelFetch(iChannel0, pos, 0);

    float emissiveness = 0.0;

    float opacity = color.w;
    opacity = min(opacity, 0.001);


    //noise code
//    if(color.w<0.99){
//        opacity = 0.0;
//    }else{
//        opacity = 1.0;
//        if(length(color.xyz) > 1.4){
//            emissiveness = 100.0;
//        }else{
//            color.xyz = vec3(1.0,0.0,1.0);
//        }
//
//    }

    float packedColor = PACK4U(color,vec2(0.0,1.0));

    fragColor = vec4(packedColor,emissiveness,opacity,1.0);
}