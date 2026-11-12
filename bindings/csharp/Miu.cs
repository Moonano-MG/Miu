// Miu 2D rendering engine — C# binding for C ABI version 3.
//
// The binding is a thin, explicit projection of `include/miu/miu.h`:
//
//   * every `miu_*_t` structure becomes a `[StructLayout(LayoutKind.Sequential)]` type
//     with the same field order, so blittable marshalling is exact;
//   * every function is declared once, in `Native`, with the ABI's calling
//     convention and integer widths;
//   * every opaque handle is wrapped in an `IDisposable` class that owns one native
//     handle and never copies it.
//
// The ABI version is checked on first use of `Native.AbiVersion`; call
// `MiuRuntime.VerifyAbi()` at start-up to fail fast with a clear message instead of
// crashing later.

using System;
using System.Runtime.InteropServices;

namespace Miu;

/// <summary>Native library name; overridden by <c>DllImportResolver</c> if needed.</summary>
internal static class Library
{
    internal const string Name = "miu";
}

/// <summary>Status codes returned by the C ABI.</summary>
public enum MiuStatus : uint
{
    /// <summary>Success.</summary>
    Ok = 0,
    /// <summary>Null, out-of-range, or inconsistent argument.</summary>
    InvalidArgument = 1,
    /// <summary>Device, backend, or format cannot honour the request.</summary>
    Unsupported = 2,
    /// <summary>Allocation failed.</summary>
    OutOfMemory = 3,
    /// <summary>Resource not found.</summary>
    NotFound = 4,
    /// <summary>File or stream error.</summary>
    Io = 5,
    /// <summary>Encoded image could not be decoded.</summary>
    Decode = 6,
    /// <summary>Document could not be parsed.</summary>
    Parse = 7,
    /// <summary>Object is in a state that forbids the operation.</summary>
    InvalidState = 8,
    /// <summary>Configured limit exceeded.</summary>
    LimitExceeded = 9,
    /// <summary>Font could not be parsed.</summary>
    Font = 10,
    /// <summary>Camera or transform was not finite or invertible.</summary>
    Geometry = 11,
    /// <summary>ABI version mismatch.</summary>
    AbiMismatch = 12,
}

/// <summary>Operating system or browser family.</summary>
public enum Platform : byte
{
    /// <summary>Microsoft Windows.</summary>
    Windows = 1,
    /// <summary>Android.</summary>
    Android = 2,
    /// <summary>macOS.</summary>
    MacOS = 3,
    /// <summary>iOS or iPadOS.</summary>
    IOS = 4,
    /// <summary>Linux.</summary>
    Linux = 5,
    /// <summary>Browser or WebAssembly host.</summary>
    Web = 6,
}

/// <summary>Graphics API profile.</summary>
public enum Backend : byte
{
    /// <summary>Direct2D 1.2.</summary>
    Direct2D12 = 1,
    /// <summary>Direct3D 11.3.</summary>
    Direct3D113 = 2,
    /// <summary>Direct3D 12.</summary>
    Direct3D12 = 3,
    /// <summary>OpenGL ES 3.0.</summary>
    OpenGLES30 = 4,
    /// <summary>OpenGL ES 2.0 (emergency).</summary>
    OpenGLES20 = 5,
    /// <summary>Metal 2.</summary>
    Metal2 = 6,
    /// <summary>Metal 3.</summary>
    Metal3 = 7,
    /// <summary>Metal 1 (emergency).</summary>
    Metal1 = 8,
    /// <summary>Vulkan 1.1.</summary>
    Vulkan11 = 9,
    /// <summary>Vulkan 1.2.</summary>
    Vulkan12 = 10,
    /// <summary>Vulkan 1.3.</summary>
    Vulkan13 = 11,
    /// <summary>Vulkan 1.4.</summary>
    Vulkan14 = 12,
    /// <summary>Desktop OpenGL 3.3 (emergency).</summary>
    OpenGL33 = 13,
    /// <summary>Desktop OpenGL 3.0 (emergency).</summary>
    OpenGL30 = 14,
    /// <summary>WebGL 2.</summary>
    WebGL2 = 15,
    /// <summary>WebGL 1 (emergency).</summary>
    WebGL1 = 16,
    /// <summary>WebGPU.</summary>
    WebGPU = 17,
}

/// <summary>Feature tier.</summary>
public enum Tier : byte
{
    /// <summary>Compatibility tier: fastest APIs, lowest requirements.</summary>
    Compatibility = 1,
    /// <summary>Balanced tier.</summary>
    Balanced = 2,
    /// <summary>Premium tier: best image quality.</summary>
    Premium = 3,
}

/// <summary>Quality preset.</summary>
public enum Preset : byte
{
    /// <summary>90% performance, 10% quality.</summary>
    Low = 1,
    /// <summary>70% performance, 30% quality.</summary>
    Medium = 2,
    /// <summary>40% performance, 60% quality.</summary>
    High = 3,
    /// <summary>10% performance, 90% quality.</summary>
    Unlimited = 4,
}

/// <summary>Compositing mode.</summary>
public enum BlendMode : byte
{
    /// <summary>Standard alpha blending.</summary>
    SourceOver = 0,
    /// <summary>Additive blending.</summary>
    Add = 1,
    /// <summary>Multiplicative blending.</summary>
    Multiply = 2,
    /// <summary>Screen blending.</summary>
    Screen = 3,
    /// <summary>Replace the destination.</summary>
    Copy = 4,
}

/// <summary>Horizontal text alignment.</summary>
public enum TextAlign : byte
{
    /// <summary>Left aligned.</summary>
    Left = 0,
    /// <summary>Centred.</summary>
    Center = 1,
    /// <summary>Right aligned.</summary>
    Right = 2,
}

/// <summary>Line-breaking strategy.</summary>
public enum TextWrap : byte
{
    /// <summary>Never wrap.</summary>
    None = 0,
    /// <summary>Break at spaces.</summary>
    Word = 1,
    /// <summary>Break at any character.</summary>
    Character = 2,
}

/// <summary>Asset category.</summary>
public enum AssetKind : uint
{
    /// <summary>Image asset.</summary>
    Texture = 0,
    /// <summary>TrueType font asset.</summary>
    Font = 1,
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Two-dimensional vector.</summary>
public struct Vec2
{
    /// <summary>Horizontal component.</summary>
    public float X;
    /// <summary>Vertical component.</summary>
    public float Y;

    /// <summary>Creates a vector.</summary>
    public Vec2(float x, float y)
    {
        X = x;
        Y = y;
    }
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Straight-alpha colour, components in [0, 1].</summary>
public struct Color
{
    /// <summary>Red.</summary>
    public float R;
    /// <summary>Green.</summary>
    public float G;
    /// <summary>Blue.</summary>
    public float B;
    /// <summary>Alpha.</summary>
    public float A;

    /// <summary>Creates a colour.</summary>
    public Color(float r, float g, float b, float a = 1f)
    {
        R = r;
        G = g;
        B = b;
        A = a;
    }

    /// <summary>White.</summary>
    public static Color White => new(1f, 1f, 1f, 1f);

    /// <summary>Transparent black.</summary>
    public static Color Transparent => new(0f, 0f, 0f, 0f);

    /// <summary>Creates a colour from 8-bit components.</summary>
    public static Color FromBytes(byte r, byte g, byte b, byte a = 255) =>
        new(r / 255f, g / 255f, b / 255f, a / 255f);
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Premultiplied 8-bit texel.</summary>
public struct Rgba8
{
    /// <summary>Red.</summary>
    public byte R;
    /// <summary>Green.</summary>
    public byte G;
    /// <summary>Blue.</summary>
    public byte B;
    /// <summary>Alpha.</summary>
    public byte A;

    /// <summary>Creates a texel.</summary>
    public Rgba8(byte r, byte g, byte b, byte a)
    {
        R = r;
        G = g;
        B = b;
        A = a;
    }

    /// <summary>Opaque black.</summary>
    public static Rgba8 Black => new(0, 0, 0, 255);
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Axis-aligned rectangle.</summary>
public struct Rect
{
    /// <summary>Left edge.</summary>
    public float X;
    /// <summary>Top edge.</summary>
    public float Y;
    /// <summary>Width, never negative.</summary>
    public float Width;
    /// <summary>Height, never negative.</summary>
    public float Height;

    /// <summary>Creates a rectangle.</summary>
    public Rect(float x, float y, float width, float height)
    {
        X = x;
        Y = y;
        Width = width;
        Height = height;
    }
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Translation, rotation, and scale.</summary>
public struct Transform2D
{
    /// <summary>Translation.</summary>
    public Vec2 Translation;
    /// <summary>Rotation in radians.</summary>
    public float RotationRadians;
    /// <summary>Scale.</summary>
    public Vec2 Scale;

    /// <summary>Identity transform.</summary>
    public static Transform2D Identity => new()
    {
        Translation = new Vec2(0f, 0f),
        RotationRadians = 0f,
        Scale = new Vec2(1f, 1f),
    };

    /// <summary>Creates a transform.</summary>
    public static Transform2D Create(Vec2 translation, float rotationRadians, Vec2 scale) => new()
    {
        Translation = translation,
        RotationRadians = rotationRadians,
        Scale = scale,
    };
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Independent 2D camera.</summary>
public struct Camera2D
{
    /// <summary>World position at the centre of the viewport.</summary>
    public Vec2 Position;
    /// <summary>Viewport size in pixels.</summary>
    public Vec2 Viewport;
    /// <summary>Zoom factor.</summary>
    public float Zoom;
    /// <summary>Rotation in radians.</summary>
    public float RotationRadians;

    /// <summary>Camera framed to a viewport at unit zoom, centred on the origin.</summary>
    public static Camera2D ForViewport(float width, float height) => new()
    {
        Position = new Vec2(width * 0.5f, height * 0.5f),
        Viewport = new Vec2(width, height),
        Zoom = 1f,
        RotationRadians = 0f,
    };
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Atlas source rectangle; all zero means the whole texture.</summary>
public struct SourceRect
{
    /// <summary>Left edge in texels.</summary>
    public uint X;
    /// <summary>Top edge in texels.</summary>
    public uint Y;
    /// <summary>Width in texels.</summary>
    public uint Width;
    /// <summary>Height in texels.</summary>
    public uint Height;

    /// <summary>Creates a source rectangle.</summary>
    public SourceRect(uint x, uint y, uint width, uint height)
    {
        X = x;
        Y = y;
        Width = width;
        Height = height;
    }
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>One sprite to draw.</summary>
public struct Sprite
{
    /// <summary>Texture id registered with the renderer.</summary>
    public ulong Texture;
    /// <summary>Local transform.</summary>
    public Transform2D Transform;
    /// <summary>Atlas rectangle.</summary>
    public SourceRect Source;
    /// <summary>Left edge of the sampling rectangle, normalised.</summary>
    public float Uv0;
    /// <summary>Top edge of the sampling rectangle, normalised.</summary>
    public float Uv1;
    /// <summary>Right edge of the sampling rectangle, normalised.</summary>
    public float Uv2;
    /// <summary>Bottom edge of the sampling rectangle, normalised.</summary>
    public float Uv3;
    /// <summary>Destination size; zero uses the source size.</summary>
    public Vec2 Size;
    /// <summary>Normalised pivot.</summary>
    public Vec2 Pivot;
    /// <summary>Premultiplied tint.</summary>
    public Color Color;
    /// <summary>Painter layer.</summary>
    public int Layer;
    /// <summary>Blend mode.</summary>
    public BlendMode Blend;

    /// <summary>Default sprite: identity transform, whole texture, white.</summary>
    public static Sprite Default => new()
    {
        Texture = 0,
        Transform = Transform2D.Identity,
        Source = default,
        Uv0 = 0f,
        Uv1 = 0f,
        Uv2 = 1f,
        Uv3 = 1f,
        Size = default,
        Pivot = default,
        Color = Color.White,
        Layer = 0,
        Blend = BlendMode.SourceOver,
    };
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Text layout parameters.</summary>
public struct TextStyle
{
    /// <summary>Em size in pixels.</summary>
    public float FontSize;
    /// <summary>Line-height multiplier.</summary>
    public float LineHeight;
    /// <summary>Extra spacing between glyphs.</summary>
    public float LetterSpacing;
    /// <summary>Horizontal alignment.</summary>
    public TextAlign Align;
    /// <summary>Wrapping strategy.</summary>
    public TextWrap Wrap;
    private byte reserved0;
    private byte reserved1;

    /// <summary>Default style: 16 px, 1.2 line height, left aligned, word wrapped.</summary>
    public static TextStyle Default => new()
    {
        FontSize = 16f,
        LineHeight = 1.2f,
        LetterSpacing = 0f,
        Align = TextAlign.Left,
        Wrap = TextWrap.Word,
    };
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Renderer selection request.</summary>
public struct RenderRequest
{
    /// <summary>Lowest tier the project supports.</summary>
    public Tier MinimumTier;
    /// <summary>Tier tried first.</summary>
    public Tier PreferredTier;
    /// <summary>Requested preset.</summary>
    public Preset Preset;
    /// <summary>Non-zero allows emergency APIs and below-minimum plans.</summary>
    public byte BestEffort;
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Selected render plan.</summary>
public struct RenderPlan
{
    /// <summary>API the bridge must create.</summary>
    public Backend Backend;
    /// <summary>API profile that implied the tier.</summary>
    public Backend ProfileBackend;
    /// <summary>Selected tier.</summary>
    public Tier Tier;
    /// <summary>Preset after clamping.</summary>
    public Preset Preset;
    /// <summary>Bit 0: emergency; bit 1: below the declared minimum.</summary>
    public uint Flags;

    /// <summary>Whether the plan uses an emergency API.</summary>
    public bool Emergency => (Flags & 1u) != 0;

    /// <summary>Whether the plan is below the declared minimum tier.</summary>
    public bool BelowMinimum => (Flags & 2u) != 0;
}

[StructLayout(LayoutKind.Sequential)]
/// <summary>Per-frame counters.</summary>
public struct RenderStats
{
    /// <summary>Sprites drawn.</summary>
    public uint Sprites;
    /// <summary>Paths filled.</summary>
    public uint Fills;
    /// <summary>Paths stroked.</summary>
    public uint Strokes;
    /// <summary>Glyphs drawn.</summary>
    public uint Glyphs;
    /// <summary>Operations culled or clipped.</summary>
    public uint Skipped;
    /// <summary>Glyphs drawn without the mask cache.</summary>
    public uint UncachedGlyphs;
    /// <summary>Estimated covered pixels.</summary>
    public ulong CoveredPixels;
}

/// <summary>Failure raised by the binding; carries the native status code.</summary>
public sealed class MiuException : Exception
{
    /// <summary>Creates an exception for a failing call.</summary>
    public MiuException(MiuStatus status, string action)
        : base($"{action}: {Native.StatusMessage(status)} ({(uint)status})")
    {
        Status = status;
    }

    /// <summary>Native status code.</summary>
    public MiuStatus Status { get; }
}

/// <summary>Raw P/Invoke surface. One declaration per exported C function.</summary>
public static class Native
{
    private const string L = Library.Name;

    [DllImport(L)] private static extern uint miu_abi_version();
    [DllImport(L)] private static extern uint miu_abi_min_supported_version();
    [DllImport(L)] private static extern IntPtr miu_version_string();
    [DllImport(L)] private static extern IntPtr miu_status_message(uint status);
    [DllImport(L)] private static extern IntPtr miu_platform_name(byte platform);
    [DllImport(L)] private static extern IntPtr miu_backend_name(byte backend);

    [DllImport(L)]
    private static extern MiuStatus miu_select_renderer(byte platform, byte[]? backends, UIntPtr count,
                                                        in RenderRequest request, out RenderPlan plan);

    [DllImport(L)]
    private static extern IntPtr miu_plan_warning(in RenderPlan plan);

    [DllImport(L)]
    private static extern MiuStatus miu_texture_create(uint width, uint height, byte[] pixels,
                                                       out IntPtr texture);

    [DllImport(L)]
    private static extern MiuStatus miu_texture_create_straight(uint width, uint height, byte[] pixels,
                                                                out IntPtr texture);

    [DllImport(L)]
    private static extern MiuStatus miu_texture_load(byte[] bytes, UIntPtr length, out IntPtr texture);

    [DllImport(L)]
    private static extern MiuStatus miu_texture_size(IntPtr texture, out uint width, out uint height);

    [DllImport(L)]
    private static extern MiuStatus miu_texture_read(IntPtr texture, byte[] buffer, UIntPtr capacity);

    [DllImport(L)] private static extern void miu_texture_release(IntPtr texture);

    [DllImport(L)]
    private static extern MiuStatus miu_font_load(byte[] bytes, UIntPtr length, out IntPtr font);

    [DllImport(L)]
    private static extern MiuStatus miu_font_metrics(IntPtr font, float fontSize, out float lineHeight,
                                                     out float ascent, out float descent);

    [DllImport(L)]
    private static extern MiuStatus miu_font_measure(IntPtr font, byte[] text, UIntPtr length,
                                                     in TextStyle style, byte hasMaxWidth, float maxWidth,
                                                     out Vec2 size);

    [DllImport(L)] private static extern uint miu_font_glyph_count(IntPtr font);
    [DllImport(L)] private static extern void miu_font_release(IntPtr font);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_create(uint width, uint height, Preset preset,
                                                        out IntPtr renderer);

    [DllImport(L)] private static extern void miu_renderer_release(IntPtr renderer);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_size(IntPtr renderer, out uint width, out uint height);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_resize(IntPtr renderer, uint width, uint height);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_set_preset(IntPtr renderer, Preset preset);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_upload_texture(IntPtr renderer, ulong id, IntPtr texture);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_upload_font(IntPtr renderer, ulong id, IntPtr font);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_set_clip(IntPtr renderer, byte hasClip, Rect clip);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_begin_frame(IntPtr renderer, byte hasClear, Rgba8 clear);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_draw_sprite(IntPtr renderer, in Camera2D camera,
                                                             in Sprite sprite);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_fill_rect(IntPtr renderer, in Camera2D camera, Rect rect,
                                                           Color color, BlendMode blend);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_draw_text(IntPtr renderer, in Camera2D camera, ulong fontId,
                                                           byte[] text, UIntPtr length, in TextStyle style,
                                                           Color color, Transform2D transform, BlendMode blend,
                                                           byte hasMaxWidth, float maxWidth);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_stats(IntPtr renderer, out RenderStats stats);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_read_pixels(IntPtr renderer, byte[] buffer, UIntPtr capacity);

    [DllImport(L)]
    private static extern MiuStatus miu_renderer_encode_png(IntPtr renderer, byte[] buffer, UIntPtr capacity,
                                                            out UIntPtr length);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_create(float width, float height, out IntPtr scene);

    [DllImport(L)] private static extern void miu_scene_release(IntPtr scene);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_attach(IntPtr scene, IntPtr renderer);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_set_background(IntPtr scene, byte hasBackground, Rgba8 color);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_set_camera(IntPtr scene, in Camera2D camera);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_add_group(IntPtr scene, uint parent, byte[]? name, Vec2 position,
                                                        out uint id);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_add_sprite(IntPtr scene, uint parent, byte[]? name, ulong texture,
                                                         Vec2 size, Vec2 pivot, Vec2 position, Color color,
                                                         BlendMode blend, int layer, out uint id);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_add_text(IntPtr scene, uint parent, byte[]? name, ulong font,
                                                       byte[] text, UIntPtr length, in TextStyle style,
                                                       Color color, Vec2 position, int layer, out uint id);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_add_rect(IntPtr scene, uint parent, byte[]? name, Rect rect,
                                                       Color color, int layer, out uint id);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_set_transform(IntPtr scene, uint node, in Transform2D transform);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_set_visible(IntPtr scene, uint node, byte visible);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_set_opacity(IntPtr scene, uint node, float opacity);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_set_layer(IntPtr scene, uint node, int layer);

    [DllImport(L)] private static extern MiuStatus miu_scene_remove(IntPtr scene, uint node);

    [DllImport(L)] private static extern MiuStatus miu_scene_node_count(IntPtr scene, out uint count);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_animation_count(IntPtr scene, out uint count);

    [DllImport(L)] private static extern MiuStatus miu_scene_update(IntPtr scene, float timeSeconds);

    [DllImport(L)]
    private static extern MiuStatus miu_scene_hit_test(IntPtr scene, Vec2 world, out uint id);

    [DllImport(L)] private static extern MiuStatus miu_scene_draw(IntPtr scene, IntPtr renderer);

    [DllImport(L)]
    private static extern MiuStatus miu_document_parse(byte[] bytes, UIntPtr length, out IntPtr document);

    [DllImport(L)] private static extern void miu_document_release(IntPtr document);

    [DllImport(L)]
    private static extern MiuStatus miu_document_viewport(IntPtr document, out Vec2 size);

    [DllImport(L)]
    private static extern MiuStatus miu_document_asset_count(IntPtr document, out uint count);

    [DllImport(L)]
    private static extern MiuStatus miu_document_asset_path(IntPtr document, uint index, byte[] buffer,
                                                            UIntPtr capacity, out UIntPtr length);

    [DllImport(L)]
    private static extern MiuStatus miu_document_asset_kind(IntPtr document, uint index, out AssetKind kind);

    [DllImport(L)]
    private static extern MiuStatus miu_document_set_texture(IntPtr document, byte[]? path, IntPtr texture);

    [DllImport(L)]
    private static extern MiuStatus miu_document_set_font(IntPtr document, byte[]? path, IntPtr font);

    [DllImport(L)]
    private static extern MiuStatus miu_document_instantiate(IntPtr document, out IntPtr scene);

    /// <summary>ABI version of the loaded library.</summary>
    public static uint AbiVersion => miu_abi_version();

    /// <summary>Oldest ABI version the library accepts.</summary>
    public static uint AbiMinSupportedVersion => miu_abi_min_supported_version();

    /// <summary>Engine version string.</summary>
    public static string VersionString() => Marshal.PtrToStringUTF8(miu_version_string()) ?? string.Empty;

    /// <summary>English description of a status code.</summary>
    public static string StatusMessage(MiuStatus status) =>
        Marshal.PtrToStringUTF8(miu_status_message((uint)status)) ?? "unknown status";

    /// <summary>Identifier of a backend, or an empty string when unknown.</summary>
    public static string BackendName(Backend backend) =>
        Marshal.PtrToStringUTF8(miu_backend_name((byte)backend)) ?? string.Empty;

    /// <summary>Identifier of a platform, or an empty string when unknown.</summary>
    public static string PlatformName(Platform platform) =>
        Marshal.PtrToStringUTF8(miu_platform_name((byte)platform)) ?? string.Empty;

    /// <summary>Selects a render plan for a device.</summary>
    public static MiuStatus SelectRenderer(Platform platform, Backend[] backends, in RenderRequest request,
                                           out RenderPlan plan)
    {
        byte[]? raw = null;
        if (backends is { Length: > 0 })
        {
            raw = new byte[backends.Length];
            for (int index = 0; index < backends.Length; index++)
            {
                raw[index] = (byte)backends[index];
            }
        }
        return miu_select_renderer((byte)platform, raw, (UIntPtr)(raw?.Length ?? 0), request, out plan);
    }

    /// <summary>Warning a host must show for a plan, or an empty string.</summary>
    public static string PlanWarning(in RenderPlan plan) =>
        Marshal.PtrToStringUTF8(miu_plan_warning(plan)) ?? string.Empty;

    internal static MiuStatus CreateTexture(uint width, uint height, byte[] pixels, bool straightAlpha,
                                           out IntPtr texture) =>
        straightAlpha
            ? miu_texture_create_straight(width, height, pixels, out texture)
            : miu_texture_create(width, height, pixels, out texture);

    internal static MiuStatus LoadTexture(byte[] bytes, out IntPtr texture) =>
        miu_texture_load(bytes, (UIntPtr)bytes.Length, out texture);

    internal static MiuStatus TextureSize(IntPtr texture, out uint width, out uint height) =>
        miu_texture_size(texture, out width, out height);

    internal static MiuStatus ReadTexture(IntPtr texture, byte[] buffer) =>
        miu_texture_read(texture, buffer, (UIntPtr)buffer.Length);

    internal static void ReleaseTexture(IntPtr texture) => miu_texture_release(texture);

    internal static MiuStatus LoadFont(byte[] bytes, out IntPtr font) =>
        miu_font_load(bytes, (UIntPtr)bytes.Length, out font);

    internal static MiuStatus FontMetrics(IntPtr font, float size, out float lineHeight, out float ascent,
                                          out float descent) =>
        miu_font_metrics(font, size, out lineHeight, out ascent, out descent);

    internal static MiuStatus MeasureFont(IntPtr font, byte[] text, in TextStyle style, float maxWidth,
                                          out Vec2 size) =>
        miu_font_measure(font, text, (UIntPtr)text.Length, style, maxWidth > 0f ? (byte)1 : (byte)0, maxWidth,
                         out size);

    internal static uint FontGlyphCount(IntPtr font) => miu_font_glyph_count(font);

    internal static void ReleaseFont(IntPtr font) => miu_font_release(font);

    internal static MiuStatus CreateRenderer(uint width, uint height, Preset preset, out IntPtr renderer) =>
        miu_renderer_create(width, height, preset, out renderer);

    internal static void ReleaseRenderer(IntPtr renderer) => miu_renderer_release(renderer);

    internal static MiuStatus RendererSize(IntPtr renderer, out uint width, out uint height) =>
        miu_renderer_size(renderer, out width, out height);

    internal static MiuStatus RendererResize(IntPtr renderer, uint width, uint height) =>
        miu_renderer_resize(renderer, width, height);

    internal static MiuStatus RendererSetPreset(IntPtr renderer, Preset preset) =>
        miu_renderer_set_preset(renderer, preset);

    internal static MiuStatus UploadTexture(IntPtr renderer, ulong id, IntPtr texture) =>
        miu_renderer_upload_texture(renderer, id, texture);

    internal static MiuStatus UploadFont(IntPtr renderer, ulong id, IntPtr font) =>
        miu_renderer_upload_font(renderer, id, font);

    internal static MiuStatus SetClip(IntPtr renderer, bool hasClip, Rect clip) =>
        miu_renderer_set_clip(renderer, hasClip ? (byte)1 : (byte)0, clip);

    internal static MiuStatus BeginFrame(IntPtr renderer, bool clear, Rgba8 color) =>
        miu_renderer_begin_frame(renderer, clear ? (byte)1 : (byte)0, color);

    internal static MiuStatus DrawSprite(IntPtr renderer, in Camera2D camera, in Sprite sprite) =>
        miu_renderer_draw_sprite(renderer, camera, sprite);

    internal static MiuStatus FillRect(IntPtr renderer, in Camera2D camera, Rect rect, Color color,
                                       BlendMode blend) =>
        miu_renderer_fill_rect(renderer, camera, rect, color, blend);

    internal static MiuStatus DrawText(IntPtr renderer, in Camera2D camera, ulong fontId, byte[] text,
                                       in TextStyle style, Color color, Transform2D transform,
                                       BlendMode blend, float maxWidth) =>
        miu_renderer_draw_text(renderer, camera, fontId, text, (UIntPtr)text.Length, style, color, transform,
                               blend, maxWidth > 0f ? (byte)1 : (byte)0, maxWidth);

    internal static MiuStatus GetStats(IntPtr renderer, out RenderStats stats) =>
        miu_renderer_stats(renderer, out stats);

    internal static MiuStatus ReadPixels(IntPtr renderer, byte[] buffer) =>
        miu_renderer_read_pixels(renderer, buffer, (UIntPtr)buffer.Length);

    internal static MiuStatus EncodePng(IntPtr renderer, byte[] buffer, out UIntPtr length) =>
        miu_renderer_encode_png(renderer, buffer, (UIntPtr)buffer.Length, out length);

    internal static MiuStatus CreateScene(float width, float height, out IntPtr scene) =>
        miu_scene_create(width, height, out scene);

    internal static void ReleaseScene(IntPtr scene) => miu_scene_release(scene);

    internal static MiuStatus AttachScene(IntPtr scene, IntPtr renderer) => miu_scene_attach(scene, renderer);

    internal static MiuStatus SetBackground(IntPtr scene, bool hasColor, Rgba8 color) =>
        miu_scene_set_background(scene, hasColor ? (byte)1 : (byte)0, color);

    internal static MiuStatus SetCamera(IntPtr scene, in Camera2D camera) => miu_scene_set_camera(scene, camera);

    internal static MiuStatus AddGroup(IntPtr scene, uint parent, byte[]? name, Vec2 position, out uint id) =>
        miu_scene_add_group(scene, parent, name, position, out id);

    internal static MiuStatus AddSprite(IntPtr scene, uint parent, byte[]? name, ulong texture, Vec2 size,
                                        Vec2 pivot, Vec2 position, Color color, BlendMode blend, int layer,
                                        out uint id) =>
        miu_scene_add_sprite(scene, parent, name, texture, size, pivot, position, color, blend, layer, out id);

    internal static MiuStatus AddText(IntPtr scene, uint parent, byte[]? name, ulong font, byte[] text,
                                      in TextStyle style, Color color, Vec2 position, int layer, out uint id) =>
        miu_scene_add_text(scene, parent, name, font, text, (UIntPtr)text.Length, style, color, position, layer,
                           out id);

    internal static MiuStatus AddRect(IntPtr scene, uint parent, byte[]? name, Rect rect, Color color, int layer,
                                      out uint id) =>
        miu_scene_add_rect(scene, parent, name, rect, color, layer, out id);

    internal static MiuStatus SetTransform(IntPtr scene, uint node, in Transform2D transform) =>
        miu_scene_set_transform(scene, node, transform);

    internal static MiuStatus SetVisible(IntPtr scene, uint node, bool visible) =>
        miu_scene_set_visible(scene, node, visible ? (byte)1 : (byte)0);

    internal static MiuStatus SetOpacity(IntPtr scene, uint node, float opacity) =>
        miu_scene_set_opacity(scene, node, opacity);

    internal static MiuStatus SetLayer(IntPtr scene, uint node, int layer) =>
        miu_scene_set_layer(scene, node, layer);

    internal static MiuStatus RemoveNode(IntPtr scene, uint node) => miu_scene_remove(scene, node);

    internal static MiuStatus NodeCount(IntPtr scene, out uint count) => miu_scene_node_count(scene, out count);

    internal static MiuStatus AnimationCount(IntPtr scene, out uint count) =>
        miu_scene_animation_count(scene, out count);

    internal static MiuStatus UpdateScene(IntPtr scene, float time) => miu_scene_update(scene, time);

    internal static MiuStatus HitTest(IntPtr scene, Vec2 world, out uint id) => miu_scene_hit_test(scene, world, out id);

    internal static MiuStatus DrawScene(IntPtr scene, IntPtr renderer) => miu_scene_draw(scene, renderer);

    internal static MiuStatus ParseDocument(byte[] bytes, out IntPtr document) =>
        miu_document_parse(bytes, (UIntPtr)bytes.Length, out document);

    internal static void ReleaseDocument(IntPtr document) => miu_document_release(document);

    internal static MiuStatus DocumentViewport(IntPtr document, out Vec2 size) =>
        miu_document_viewport(document, out size);

    internal static MiuStatus DocumentAssetCount(IntPtr document, out uint count) =>
        miu_document_asset_count(document, out count);

    internal static MiuStatus DocumentAssetPath(IntPtr document, uint index, byte[] buffer, out UIntPtr length) =>
        miu_document_asset_path(document, index, buffer, (UIntPtr)buffer.Length, out length);

    internal static MiuStatus DocumentAssetKind(IntPtr document, uint index, out AssetKind kind) =>
        miu_document_asset_kind(document, index, out kind);

    internal static MiuStatus DocumentSetTexture(IntPtr document, byte[]? path, IntPtr texture) =>
        miu_document_set_texture(document, path, texture);

    internal static MiuStatus DocumentSetFont(IntPtr document, byte[]? path, IntPtr font) =>
        miu_document_set_font(document, path, font);

    internal static MiuStatus InstantiateDocument(IntPtr document, out IntPtr scene) =>
        miu_document_instantiate(document, out scene);
}

/// <summary>Start-up helpers.</summary>
public static class MiuRuntime
{
    /// <summary>ABI version this binding was written against.</summary>
    public const uint ExpectedAbiVersion = 3;

    /// <summary>Throws when the loaded library does not speak this binding's ABI.</summary>
    public static void VerifyAbi()
    {
        uint version = Native.AbiVersion;
        if (version != ExpectedAbiVersion)
        {
            throw new MiuException(MiuStatus.AbiMismatch,
                                   $"loaded ABI {version}, binding expects {ExpectedAbiVersion}");
        }
    }

    /// <summary>Engine version string, for logs and settings screens.</summary>
    public static string Version => Native.VersionString();
}

/// <summary>Packs a managed string as NUL-terminated UTF-8 for the C ABI.</summary>
internal static class Utf8
{
    internal static byte[] NullTerminated(string text)
    {
        byte[] body = System.Text.Encoding.UTF8.GetBytes(text);
        byte[] buffer = new byte[body.Length + 1];
        Array.Copy(body, buffer, body.Length);
        return buffer;
    }

    internal static byte[] Bytes(string text) => System.Text.Encoding.UTF8.GetBytes(text);
}
