#version 460
//#pragma optimize(off)
//#pragma debug(on)

//LAYOUT INDEXES

//IMAGE BINDINGS
#define FB0TEX          0
#define COMPUTEACCUR    1
#define COMPUTEACCUG    2
#define COMPUTEACCUB    3


//UNIFORM LOCATIONS
#define FRAME           0
#define LIGHTCOUNT      1
#define TIMEOFDAY       2
#define SUNMODE         3
#define REALTIME        4
#define RAYSTOEMIT      5
#define DEBUGFLOAT0     6
#define DEBUGFLOAT1     7
#define DEBUGFLOAT2     8


//SSBO BINDINGS
#define SCENE           0
#define LIGHTLIST       1
#define SORTEDRAYS      2
#define TILESTART       3
#define TILECOUNT       4
#define TILEWRITECURSOR 5

#define INPUTQUEUE      6
#define INPUTCOUNT      7
#define OUTPUTQUEUE     8
#define OUTPUTCOUNT     9

//LAYOUT INDEXES END

layout(location = DEBUGFLOAT0) uniform float debugFloat0;
layout(location = DEBUGFLOAT1) uniform float debugFloat1;
layout(location = DEBUGFLOAT2) uniform float debugFloat2;

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

// Wavefront tile-bucket constants
#define TILE_W 16
#define TILE_H 16
#define TILES_PER_ROW int((RESOLUTIONX + TILE_W - 1) / TILE_W)
#define TILES_PER_COL int((RESOLUTIONY + TILE_H - 1) / TILE_H)
#define TILE_COUNT (TILES_PER_ROW * TILES_PER_COL)
#define TILE_PIXELS (TILE_W * TILE_H)
#define MAX_RAYS 80000

uint tile_id_of(vec2 p, vec2 dir) {
    ivec2 mp = ivec2(floor(p.x), floor(p.y));
    if(float(mp.x)==p.x && dir.x<0){
        mp.x-=1;
    }
    if(float(mp.y)==p.y && dir.y<0){
        mp.y-=1;
    }
    return uint(mp.x / TILE_W) + (TILES_PER_ROW * uint(mp.y / TILE_H));
}

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

//b5
    float thickness;
    float padding_0;
    float padding_1;
    float padding_2;
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
    uint type_and_tile_id;
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

vec2 fibonacci_lattice(int i, float N)
{
    return vec2((float(i)+0.5)/N, mod(float(i)/PHI, 1.));
}

vec3 fibonacciDecode(int i, float N)
{
    vec2 xy = fibonacci_lattice(i, N);
    vec2 pt = vec2(2.*PI*xy.y, acos(2.*xy.x - 1.) - PI*0.5);
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



























#define SKY_VISIBILITY_DIRECTIONS 64


const vec2 SKY_VISIBILITY_BOUNDS[64] = vec2[](
vec2(0.0000000000, 360.0000000000),
vec2(-31.9013668228, 359.5524916388),
vec2(-63.7234218213, 358.2110791315),
vec2(-95.3870503528, 355.9790974410),
vec2(-126.8135316476, 352.8620956255),
vec2(-157.9247345218, 348.8678230425),
vec2(-188.6433116230, 344.0062100830),
vec2(-218.8928917284, 338.2893434829),
vec2(-248.5982696158, 331.7314362733),
vec2(-277.6855930352, 324.3487924449),
vec2(-306.0825463176, 316.1597664133),
vec2(-333.7185301629, 307.1847173876),
vec2(-360.5248371607, 297.4459587538),
vec2(-386.4348226083, 286.9677026003),
vec2(-411.3840701994, 275.7759995228),
vec2(-435.3105521734, 263.8986738587),
vec2(-458.1547835262, 251.3652545110),
vec2(-479.8599698993, 238.2069015349),
vec2(-500.3721487795, 224.4563286691),
vec2(-519.6403236581, 210.1477220045),
vec2(-537.6165908165, 195.3166549917),
vec2(-554.2562584220, 180.0000000000),
vec2(-569.5179576393, 164.2358366471),
vec2(-583.3637454795, 148.0633571270),
vec2(-595.7591991323, 131.5227687719),
vec2(-606.6735015470, 114.6551940906),
vec2(-616.0795180480, 97.5025685315),
vec2(-623.9538637964, 80.1075362243),
vec2(-630.2769619278, 62.5133439601),
vec2(-635.0330922241, 44.7637336731),
vec2(-638.2104301960, 26.9028336911),
vec2(-639.8010764805, 8.9750490257),
vec2(-648.7761255062, 0.0000000000),
vec2(-665.1132638871, 0.0000000000),
vec2(-679.7968258972, 0.0000000000),
vec2(-692.7903058879, 0.0000000000),
vec2(-704.0614000206, 0.0000000000),
vec2(-713.5820865795, 0.0000000000),
vec2(-721.3286956376, 0.0000000000),
vec2(-727.2819679042, 0.0000000000),
vec2(-731.4271026065, 0.0000000000),
vec2(-733.7537942865, 0.0000000000),
vec2(-734.2562584220, 0.0000000000),
vec2(-732.9332458082, 0.0000000000),
vec2(-729.7880456627, 0.0000000000),
vec2(-724.8284774487, 0.0000000000),
vec2(-718.0668714342, 0.0000000000),
vec2(-709.5200380372, 0.0000000000),
vec2(-699.2092260321, 0.0000000000),
vec2(-687.1600697222, 0.0000000000),
vec2(-673.4025252086, 0.0000000000),
vec2(-657.9707959145, 0.0000000000),
vec2(-640.9032475505, 0.0000000000),
vec2(-622.2423127309, 0.0000000000),
vec2(-602.0343854801, 0.0000000000),
vec2(-580.3297058892, 0.0000000000),
vec2(-557.1822352114, 0.0000000000),
vec2(-532.6495217060, 0.0000000000),
vec2(-506.7925575643, 0.0000000000),
vec2(-479.6756272731, 0.0000000000),
vec2(-451.3661477938, 0.0000000000),
vec2(-421.9345009529, 0.0000000000),
vec2(-391.4538584617, 0.0000000000),
vec2(-360.0000000000, 0.0000000000)
);


const float SKY_DIRECTIONS[64] = float[](
3.1415926536,
3.1914592036,
3.2413257537,
3.2911923038,
3.3410588538,
3.3909254039,
3.4407919539,
3.4906585040,
3.5405250540,
3.5903916041,
3.6402581542,
3.6901247042,
3.7399912543,
3.7898578043,
3.8397243544,
3.8895909044,
3.9394574545,
3.9893240046,
4.0391905546,
4.0890571047,
4.1389236547,
4.1887902048,
4.2386567548,
4.2885233049,
4.3383898550,
4.3882564050,
4.4381229551,
4.4879895051,
4.5378560552,
4.5877226052,
4.6375891553,
4.6874557054,
4.7373222554,
4.7871888055,
4.8370553555,
4.8869219056,
4.9367884556,
4.9866550057,
5.0365215558,
5.0863881058,
5.1362546559,
5.1861212059,
5.2359877560,
5.2858543060,
5.3357208561,
5.3855874062,
5.4354539562,
5.4853205063,
5.5351870563,
5.5850536064,
5.6349201564,
5.6847867065,
5.7346532566,
5.7845198066,
5.8343863567,
5.8842529067,
5.9341194568,
5.9839860068,
6.0338525569,
6.0837191070,
6.1335856570,
6.1834522071,
6.2333187571,
6.2831853072
);


const float SKY_VISIBILITY_DIRECTION_DISTRIBUTIONS[65] = float[](
0.0000000000,
360.0000000000,
751.4538584617,
1173.3883594145,
1624.7545072083,
2104.4301344814,
2611.2226920457,
3143.8722137517,
3701.0544489630,
4281.3841548522,
4883.4185403323,
5505.6608530632,
6146.5641006137,
6804.5348965281,
7477.9374217367,
8165.0974914589,
8864.3067174910,
9573.8267555282,
10291.8936269625,
11016.7221044111,
11746.5101500738,
12479.4433958820,
13213.6996543040,
13947.4534485905,
14678.8805511970,
15406.1625191012,
16127.4912147388,
16841.0733013182,
17545.1347013389,
18237.9250072268,
18917.7218331240,
19582.8350970111,
20231.6112225173,
20880.3873480235,
21545.5006119106,
22225.2974378078,
22918.0877436957,
23622.1491437163,
24335.7312302958,
25057.0599259334,
25784.3418938376,
26515.7689964441,
27249.5227907306,
27983.7790491526,
28716.7122949608,
29446.5003406235,
30171.3288180721,
30889.3956895064,
31598.9157275436,
32298.1249535757,
32985.2850232979,
33658.6875485065,
34316.6583444209,
34957.5615919714,
35579.8039047023,
36181.8382901824,
36762.1679960716,
37319.3502312829,
37851.9997529889,
38358.7923105532,
38838.4679378263,
39289.8340856201,
39711.7685865729,
40103.2224450346,
40463.2224450346
);


