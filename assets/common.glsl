#version 460
#extension GL_EXT_gpu_shader4 : enable
#define KEEP_FRAMES 100
#define COMPUTE_MULTIPLIER 1000000

#define RESOLUTIONX 640
#define RESOLUTIONY 360
//#define RESOLUTIONX 3840
//#define RESOLUTIONY 2160

//#define LIGHT_MAX_RADIUS 64.0
//#define LIGHT_MAX_RADIUS 256.0
#define LIGHT_MAX_RADIUS 1024.0
#define LIGHT_MIN_DIST 100000.0
#define LIGHT_POWER_DISABLED 1.0
float linearFalloff = (1.0/
(LIGHT_MAX_RADIUS*
(LIGHT_MIN_DIST+(LIGHT_MAX_RADIUS*LIGHT_MAX_RADIUS))
)
);

#define MAX_LIGHTS 1024

#define FLT_MAX 3.402823466e+38
#define FLT_MIN 1.175494351e-38

struct PixelData {
// Block 1 (16 bytes)
    vec3 color;// r, g, b
    float density;

// Block 2 (16 bytes)
    float transmission;
    float emissiveness;
    float reflectiveness; // 0.0 - 1.0
    float metallic; // 0.0 - 1.0

// Block 3 (16 bytes)
    vec3 scattering;
    float roughness; // 0.0 - 1.0
    
//b4
    vec3 absorption;
    float anisotropy;
};

struct LightPoint {
    float x;
    float y;
    float padding_0;
    float padding_1;
};

struct Ray {
    vec2 position;
    vec2 direction;

    vec3 energy;
    float tile_id;
};

float lightIntensity(in float dist){
    return
    (
    (1.0/(LIGHT_MIN_DIST+(dist*dist)))
    -
    (dist*linearFalloff)
    )
    * LIGHT_MIN_DIST;
}


//mini
float noise1(float seed1,float seed2){
    return fract((
    fract(seed1+12.34567*
    fract(100.*(abs(seed1*0.91)+seed2+94.68)*
    fract((abs(seed2*0.41)+45.46)*
    fract((abs(seed2)+757.21)*
    fract(seed1*0.0171))))))
    * 1.0038 - 0.00185);
}
//2 seeds
float noise2(float seed1,float seed2){
    float buff1 = abs(seed1+100.94) + 1000.;
    float buff2 = abs(seed2+100.73) + 1000.;
    buff1 = (buff1*fract(buff2*fract(buff1*fract(buff2*0.63))));
    buff2 = (buff2*fract(buff2*fract(buff1+buff2*fract(seed1*0.79))));
    buff1 = noise1(buff1, buff2);
    return fract(buff1 * 1.0038 - 0.00185);
}

float noise2vec(vec2 seed) {
    return noise2(seed.x,seed.y);
}


//3 seeds
float noise2(float seed1,float seed2,float seed3){
    float buff1 = abs(seed1+100.81) + 1000.3;
    float buff2 = abs(seed2+100.45) + 1000.2;
    float buff3 = abs(noise1(seed1, seed2)+seed3) + 1000.1;
    buff1 = (buff3*fract(buff2*fract(buff1*fract(buff2*0.146))));
    buff2 = (buff2*fract(buff2*fract(buff1+buff2*fract(buff3*0.52))));
    buff1 = noise1(buff1, buff2);
    return(buff1);
}

//3 seeds hard
float noise3(float seed1,float seed2,float seed3){
    float buff1 = abs(seed1+100.813) + 1000.314;
    float buff2 = abs(seed2+100.453) + 1000.213;
    float buff3 = abs(noise1(buff2, buff1)+seed3) + 1000.17;
    buff1 = (buff3*fract(buff2*fract(buff1*fract(buff2*0.14619))));
    buff2 = (buff2*fract(buff2*fract(buff1+buff2*fract(buff3*0.5215))));
    buff1 = noise2(noise1(seed2,buff1), noise1(seed1,buff2), noise1(seed3,buff3));
    return(buff1);
}

//from https://www.shadertoy.com/view/wtKyR1

vec4 b127_ieee754_encode (uvec4 v) {
    v.w&=0x7FFFFFFFu;
    uvec4 l = uvec4( v.xyz&0xFFFFFFu , v.x>>24 | v.y>>24<<8 | v.z>>24<<16 );
    return uintBitsToFloat( (l&0x800000u)<<8 | l&0x7FFFFFu
    | 1u+uvec4(v.w/uvec4(1,254,254*254,254*254*254))%254u <<23 );
}

uvec4 b127_ieee754_decode (vec4 f) {
    uvec4 e = floatBitsToUint(f);
    uvec4 l = (e&0x80000000u)>>8 | e&0x7FFFFFu;
    uvec4 h = (e>>23&0xFFu)-1u;
    return uvec4(
    l.xyz | uvec3(l.w)>>uvec3(0,8,16)<<24,
    h.x+254u*h.y+(254u*254u)*h.z+(254u*254u*254u)*h.w );
}


// UINT PACKING
//8bit number packing
uint encode4to1(uvec4 x)
{
    x = min(x, uvec4(1<<8) - 1u);
    return x.w << 24 | x.z << 16 | x.y << 8 | x.x;
}

uvec4 decode1to4(uint x)
{
    return (uvec4(x) >> uvec4(0, 8, 16, 24)) & (uvec4(1<<8) - 1u);
}

//11 11 10 bit number packing
uint encode3to1(uvec3 x)
{
    x = min(x, uvec3(1)<<uvec3(11,11,10) - 1u);
    return x.z << 22 | x.y << 11 | x.x;
}

uvec3 decode1to3(uint x)
{
    return (uvec3(x) >> uvec3(0, 11, 22)) & ((uvec3(1) << uvec3(11,11,10)) - 1u);
}

//16bit number packing
uint encode2to1(uvec2 x)
{
    x = min(x, uvec2(1<<16) - 1u);
    return x.y << 16 | x.x;
}

uvec2 decode1to2(uint x)
{
    return (uvec2(x) >> uvec2(0, 16)) & (uvec2(1<<16) - 1u);
}


//UINT TO FLOAT CONVERSION

//uniform quantization
uint uquant(float x, vec2 range, int bits)
{
    return uint( exp2(float(bits)) *((x) - range.x)/(range.y - range.x) );
}

//decode
float ufloat(uint x, vec2 range, int bits)
{
    return float(x)*(range.y - range.x)/exp2(float(bits)) + range.x;
}


//signed logarithmic quantization
//to emulate floating point numbers
uint slogquant(float x, vec2 range, int bits)
{
    uint value = uint(exp2(float(bits - 1)) *(log2(abs(x)) - range.x)/(range.y - range.x));
    uint sig = uint(exp2(float(bits - 1))*sign(x));
    return value + sig;
}

//decode
float slogfloat(uint x, vec2 range, int bits)
{
    uint value = x&((1u<<(bits - 1)) - 1u);
    uint sig = x>>(bits - 1);
    return (float(sig)*2.0 - 1.0)*exp2(float(value)*(range.y - range.x)/exp2(float(bits - 1)) + range.x);
}

//unsigned logarithmic quantization
uint logquant(float x, vec2 range, int bits)
{
    return uint( exp2(float(bits)) *(log2(abs(x)) - range.x)/(range.y - range.x) );
}

//decode
float logfloat(uint x, vec2 range, int bits)
{
    return exp2(float(x)*(range.y - range.x)/exp2(float(bits)) + range.x);
}

//fibonacci uniform sphere quantization
//to store normals and other vector stuff


//from https://www.shadertoy.com/view/lllXz4
const float PI  = 3.14159265358979323846;
const float PHI = 1.61803398875;

#define M_PI 3.141592653

vec2 fibonacci_lattice(int i, float N)
{
    return vec2((float(i)+0.5)/N, mod(float(i)/PHI, 1.));
}

vec3 fibonacciDecode(int i, float N)
{
    vec2 xy = fibonacci_lattice(i, N);
    vec2 pt = vec2(2.*M_PI*xy.y, acos(2.*xy.x - 1.) - M_PI*0.5);
    return vec3(cos(pt.x)*cos(pt.y), sin(pt.x)*cos(pt.y), sin(pt.y));
}

vec2 fibonacciEncode( vec3 p, float n)
{
    float m = 1.0 - 1.0/n;

    float phi = min(atan(p.y, p.x), PI), cosTheta = p.z;

    float k  = max(2.0, floor( log(n * PI * sqrt(5.0) * (1.0 - cosTheta*cosTheta))/ log(PHI+1.0)));
    float Fk = pow(PHI, k)/sqrt(5.0);
    vec2  F  = vec2( round(Fk), round(Fk * PHI) ); // k, k+1

    vec2 ka = 2.0*F/n;
    vec2 kb = 2.0*PI*( fract((F+1.0)*PHI) - (PHI-1.0) );

    mat2 iB = mat2( ka.y, -ka.x,
    kb.y, -kb.x ) / (ka.y*kb.x - ka.x*kb.y);

    vec2 c = floor( iB * vec2(phi, cosTheta - m));
    float d = 8.0;
    float j = 0.0;
    for( int s=0; s<4; s++ )
    {
        vec2 uv = vec2( float(s-2*(s/2)), float(s/2) );

        float i = dot(F, uv + c); // all quantities are ingeters (can take a round() for extra safety)

        float phi = 2.0*PI*fract(i*PHI);
        float cosTheta = m - 2.0*i/n;
        float sinTheta = sqrt(1.0 - cosTheta*cosTheta);

        vec3 q = vec3( cos(phi)*sinTheta, sin(phi)*sinTheta, cosTheta );
        float squaredDistance = dot(q-p, q-p);
        if (squaredDistance < d)
        {
            d = squaredDistance;
            j = i;
        }
    }
    return vec2( j, sqrt(d) );
}


//final packing functions

float PACK4U(vec4 x, vec2 range)
{
    //quantize
    uvec4 quant = uvec4(uquant(x.x, range, 8),
    uquant(x.y, range, 8),
    uquant(x.z, range, 8),
    uquant(x.w, range, 8));
    //pack
    return uintBitsToFloat(encode4to1(quant));
}

vec4 UNPACK4U(float x, vec2 range)
{
    uvec4 unpack = decode1to4(floatBitsToUint(x));

    //unquantize
    return vec4(ufloat(unpack.x, range, 8),
    ufloat(unpack.y, range, 8),
    ufloat(unpack.z, range, 8),
    ufloat(unpack.w, range, 8));
}

float PACK3U(vec3 x, vec2 range)
{
    //quantize
    uvec3 quant = uvec3(uquant(x.x, range, 11),
    uquant(x.y, range, 11),
    uquant(x.z, range, 10));
    //pack
    return uintBitsToFloat(encode3to1(quant));
}

vec3 UNPACK3U(float x, vec2 range)
{
    uvec3 unpack = decode1to3(floatBitsToUint(x));

    //unquantize
    return vec3(ufloat(unpack.x, range, 11),
    ufloat(unpack.y, range, 11),
    ufloat(unpack.z, range, 10));
}

float PACK2U(vec2 x, vec2 range)
{
    //quantize
    uvec2 quant = uvec2(uquant(x.x, range, 16),
    uquant(x.y, range, 16));
    //pack
    return uintBitsToFloat(encode2to1(quant));
}

vec2 UNPACK2U(float x, vec2 range)
{
    uvec2 unpack = decode1to2(floatBitsToUint(x));

    //unquantize
    return vec2(ufloat(unpack.x, range, 16),
    ufloat(unpack.y, range, 16));
}


float PACK4LOG(vec4 x, vec2 range)
{
    //quantize
    uvec4 quant = uvec4(logquant(x.x, range, 8),
    logquant(x.y, range, 8),
    logquant(x.z, range, 8),
    logquant(x.w, range, 8));
    //pack
    return uintBitsToFloat(encode4to1(quant));
}

vec4 UNPACK4LOG(float x, vec2 range)
{
    uvec4 unpack = decode1to4(floatBitsToUint(x));

    //unquantize
    return vec4(logfloat(unpack.x, range, 8),
    logfloat(unpack.y, range, 8),
    logfloat(unpack.z, range, 8),
    logfloat(unpack.w, range, 8));
}

float PACK2LOG(vec2 x, vec2 range)
{
    //quantize
    uvec2 quant = uvec2(logquant(x.x, range, 16),
    logquant(x.y, range, 16));
    //pack
    return uintBitsToFloat(encode2to1(quant));
}

vec2 UNPACK2LOG(float x, vec2 range)
{
    uvec2 unpack = decode1to2(floatBitsToUint(x));

    //unquantize
    return vec2(logfloat(unpack.x, range, 16),
    logfloat(unpack.y, range, 16));
}

//signed log packing
float PACK2SLOG(vec2 x, vec2 range)
{
    //quantize
    uvec2 quant = uvec2(slogquant(x.x, range, 16),
    slogquant(x.y, range, 16));
    //pack
    return uintBitsToFloat(encode2to1(quant));
}

vec2 UNPACK2SLOG(float x, vec2 range)
{
    uvec2 unpack = decode1to2(floatBitsToUint(x));

    //unquantize
    return vec2(slogfloat(unpack.x, range, 16),
    slogfloat(unpack.y, range, 16));
}


//FIBONACCI VECTOR PACKING
//range here is length range
float PACKVEC3(vec3 x, vec2 range)
{
    float L = length(x);
    vec3 N = normalize(x);

    vec2 fe = fibonacciEncode(N, exp2(16.0));
    //quantize
    uvec2 quant = uvec2(fe.x, logquant(L, range, 16));
    //pack
    return uintBitsToFloat(encode2to1(quant));
}

vec3 UNPACKVEC3(float x, vec2 range)
{
    uvec2 unpack = decode1to2(floatBitsToUint(x));

    //unquantize
    vec3 N = fibonacciDecode(int(unpack.x), exp2(16.0));

    return N*logfloat(unpack.y, range, 16);
}



// NEW: Function to generate a biased random angle (Simplified 2D HG-like sampling)
float sampleBiasedAngle(float currentAngle, float g, float rngValue) {
    // This is a simplified, non-physically-based bias, but it demonstrates the concept
    // of making the new angle dependent on the old angle and the anisotropy (g).

    // Convert random value (0 to 1) to a scatter angle (-PI to PI)
    float uniformScatterAngle = (rngValue - 0.5) * 2.0 * PI;

    // Mix the uniform angle with 0.0 (perfect forward) based on the anisotropy 'g'.
    // A small 'g' means mostly uniform; a large 'g' means mostly forward.
    float biasedScatterAngle = mix(uniformScatterAngle, 0.0, abs(g));

    // Combine with the current angle
    return currentAngle + biasedScatterAngle;
}


// --- Helper: 2D Rotation ---
vec2 rotate2d(vec2 v, float a) {
    float s = sin(a);
    float c = cos(a);
    return vec2(v.x * c - v.y * s, v.x * s + v.y * c);
}



















































































