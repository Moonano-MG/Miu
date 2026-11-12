// Owning wrappers and a convenient frame loop for the CPU renderer.
//
// `Miu.cs` exposes the raw ABI. This file adds the pieces a game or tool actually
// uses: `Texture`, `Font`, `Renderer`, `Scene`, and `Document` classes that own one
// native handle and release it on `Dispose`, plus `CpuRenderer`, a small façade that
// pairs a renderer with a camera and a clip rectangle.

using System;
using System.Collections.Generic;
using System.IO;

namespace Miu;

/// <summary>Base class for types that own one native handle.</summary>
public abstract class NativeHandle : IDisposable
{
    /// <summary>Wraps an existing handle; the wrapper takes ownership.</summary>
    protected NativeHandle(IntPtr handle)
    {
        Handle = handle;
    }

    /// <summary>Native handle; never touched after disposal.</summary>
    public IntPtr Handle { get; private set; }

    /// <summary>Whether the handle is still owned.</summary>
    public bool IsValid => Handle != IntPtr.Zero;

    /// <summary>Releases the handle.</summary>
    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    /// <summary>Releases the native handle exactly once.</summary>
    protected virtual void Dispose(bool disposing)
    {
        if (Handle != IntPtr.Zero)
        {
            ReleaseNative(Handle);
            Handle = IntPtr.Zero;
        }
    }

    /// <summary>Releases the native resource behind a handle.</summary>
    protected abstract void ReleaseNative(IntPtr handle);

    /// <summary>Throws when the handle has already been released.</summary>
    protected void EnsureAlive()
    {
        if (Handle == IntPtr.Zero)
        {
            throw new ObjectDisposedException(GetType().Name);
        }
    }
}

/// <summary>Premultiplied RGBA8 texture.</summary>
public sealed class Texture : NativeHandle
{
    private Texture(IntPtr handle) : base(handle)
    {
    }

    /// <summary>Creates a texture from premultiplied RGBA8 pixels.</summary>
    public static Texture FromPremultiplied(uint width, uint height, byte[] pixels)
    {
        MiuStatus status = Native.CreateTexture(width, height, pixels, straightAlpha: false, out IntPtr handle);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "texture create");
        }
        return new Texture(handle);
    }

    /// <summary>Creates a texture from straight-alpha RGBA8 pixels.</summary>
    public static Texture FromStraight(uint width, uint height, byte[] pixels)
    {
        MiuStatus status = Native.CreateTexture(width, height, pixels, straightAlpha: true, out IntPtr handle);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "texture create straight");
        }
        return new Texture(handle);
    }

    /// <summary>Decodes a PNG, BMP, TGA, or QOI file.</summary>
    public static Texture Load(byte[] encoded) => Load(encoded, 0, encoded.Length);

    /// <summary>Decodes an image from a slice of a buffer.</summary>
    public static Texture Load(byte[] encoded, int offset, int count)
    {
        byte[] slice = encoded;
        if (offset != 0 || count != encoded.Length)
        {
            slice = new byte[count];
            Array.Copy(encoded, offset, slice, 0, count);
        }
        MiuStatus status = Native.LoadTexture(slice, out IntPtr handle);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "texture load");
        }
        return new Texture(handle);
    }

    /// <summary>Decodes an image file.</summary>
    public static Texture LoadFile(string path) => Load(File.ReadAllBytes(path));

    /// <summary>Size in pixels.</summary>
    public Vec2 Size()
    {
        EnsureAlive();
        MiuStatus status = Native.TextureSize(Handle, out uint width, out uint height);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "texture size");
        }
        return new Vec2(width, height);
    }

    /// <summary>Copies premultiplied RGBA8 pixels.</summary>
    public byte[] ReadPixels()
    {
        Vec2 size = Size();
        byte[] buffer = new byte[(int)size.X * (int)size.Y * 4];
        MiuStatus status = Native.ReadTexture(Handle, buffer);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "texture read");
        }
        return buffer;
    }

    /// <inheritdoc/>
    protected override void ReleaseNative(IntPtr handle) => Native.ReleaseTexture(handle);
}

/// <summary>Parsed TrueType font.</summary>
public sealed class Font : NativeHandle
{
    private Font(IntPtr handle) : base(handle)
    {
    }

    /// <summary>Parses a font from memory.</summary>
    public static Font Load(byte[] fontData)
    {
        MiuStatus status = Native.LoadFont(fontData, out IntPtr handle);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "font load");
        }
        return new Font(handle);
    }

    /// <summary>Parses a font file.</summary>
    public static Font LoadFile(string path) => Load(File.ReadAllBytes(path));

    /// <summary>Number of mapped glyphs.</summary>
    public uint GlyphCount()
    {
        EnsureAlive();
        return Native.FontGlyphCount(Handle);
    }

    /// <summary>Line height, ascent, and descent at <paramref name="fontSize"/> pixels.</summary>
    public (float LineHeight, float Ascent, float Descent) Metrics(float fontSize)
    {
        EnsureAlive();
        MiuStatus status = Native.FontMetrics(Handle, fontSize, out float lineHeight, out float ascent,
                                              out float descent);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "font metrics");
        }
        return (lineHeight, ascent, descent);
    }

    /// <summary>Lays text out and returns the block size.</summary>
    public Vec2 Measure(string text, TextStyle style, float maxWidth = 0f)
    {
        EnsureAlive();
        byte[] utf8 = Utf8.Bytes(text);
        MiuStatus status = Native.MeasureFont(Handle, utf8, style, maxWidth, out Vec2 size);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "font measure");
        }
        return size;
    }

    /// <inheritdoc/>
    protected override void ReleaseNative(IntPtr handle) => Native.ReleaseFont(handle);
}

/// <summary>CPU renderer with its own asset stores.</summary>
public sealed class Renderer : NativeHandle
{
    /// <summary>Creates a renderer for a framebuffer of the given size.</summary>
    public Renderer(uint width, uint height, Preset preset = Preset.High)
        : base(CreateHandle(width, height, preset))
    {
    }

    private static IntPtr CreateHandle(uint width, uint height, Preset preset)
    {
        MiuStatus status = Native.CreateRenderer(width, height, preset, out IntPtr handle);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "renderer create");
        }
        return handle;
    }

    /// <summary>Framebuffer size in pixels.</summary>
    public Vec2 Size()
    {
        EnsureAlive();
        MiuStatus status = Native.RendererSize(Handle, out uint width, out uint height);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "renderer size");
        }
        return new Vec2(width, height);
    }

    /// <summary>Resizes the framebuffer, discarding its contents.</summary>
    public void Resize(uint width, uint height) => Check(Native.RendererResize(Handle, width, height), "renderer resize");

    /// <summary>Changes the quality preset.</summary>
    public void SetPreset(Preset preset) => Check(Native.RendererSetPreset(Handle, preset), "renderer preset");

    /// <summary>Uploads a texture under an id.</summary>
    public void Upload(ulong id, Texture texture)
    {
        EnsureAlive();
        Check(Native.UploadTexture(Handle, id, texture.Handle), "upload texture");
    }

    /// <summary>Uploads a font under an id.</summary>
    public void Upload(ulong id, Font font)
    {
        EnsureAlive();
        Check(Native.UploadFont(Handle, id, font.Handle), "upload font");
    }

    /// <summary>Sets the scissor rectangle in framebuffer pixels.</summary>
    public void SetClip(Rect clip) => Check(Native.SetClip(Handle, true, clip), "set clip");

    /// <summary>Clears the scissor rectangle.</summary>
    public void ClearClip() => Check(Native.SetClip(Handle, false, default), "clear clip");

    /// <summary>Begins a frame, optionally clearing it.</summary>
    public void BeginFrame(Rgba8? clear = null) =>
        Check(Native.BeginFrame(Handle, clear.HasValue, clear ?? default), "begin frame");

    /// <summary>Draws one sprite.</summary>
    public void DrawSprite(in Camera2D camera, in Sprite sprite) =>
        Check(Native.DrawSprite(Handle, camera, sprite), "draw sprite");

    /// <summary>Fills a rectangle in world space.</summary>
    public void FillRect(in Camera2D camera, Rect rect, Color color, BlendMode blend = BlendMode.SourceOver) =>
        Check(Native.FillRect(Handle, camera, rect, color, blend), "fill rect");

    /// <summary>Draws UTF-8 text positioned by a transform.</summary>
    public void DrawText(in Camera2D camera, ulong fontId, string text, TextStyle style, Color color,
                         Transform2D transform, BlendMode blend = BlendMode.SourceOver, float maxWidth = 0f)
    {
        EnsureAlive();
        byte[] utf8 = Utf8.Bytes(text);
        Check(Native.DrawText(Handle, camera, fontId, utf8, style, color, transform, blend, maxWidth),
              "draw text");
    }

    /// <summary>Statistics for the current frame.</summary>
    public RenderStats Stats()
    {
        EnsureAlive();
        Check(Native.GetStats(Handle, out RenderStats stats), "renderer stats");
        return stats;
    }

    /// <summary>Copies the framebuffer as premultiplied RGBA8.</summary>
    public byte[] ReadPixels()
    {
        Vec2 size = Size();
        byte[] buffer = new byte[(int)size.X * (int)size.Y * 4];
        Check(Native.ReadPixels(Handle, buffer), "read pixels");
        return buffer;
    }

    /// <summary>Encodes the framebuffer as a PNG file.</summary>
    public byte[] EncodePng()
    {
        EnsureAlive();
        byte[] probe = Array.Empty<byte>();
        MiuStatus status = Native.EncodePng(Handle, probe, out UIntPtr length);
        if (status != MiuStatus.LimitExceeded && status != MiuStatus.Ok)
        {
            throw new MiuException(status, "encode png (size query)");
        }
        byte[] buffer = new byte[(int)length];
        Check(Native.EncodePng(Handle, buffer, out UIntPtr written), "encode png");
        if ((int)written != buffer.Length)
        {
            Array.Resize(ref buffer, (int)written);
        }
        return buffer;
    }

    /// <summary>Writes the frame to a PNG file.</summary>
    public void SavePng(string path) => File.WriteAllBytes(path, EncodePng());

    private static void Check(MiuStatus status, string action)
    {
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, action);
        }
    }

    /// <inheritdoc/>
    protected override void ReleaseNative(IntPtr handle) => Native.ReleaseRenderer(handle);
}

/// <summary>Scene graph with its own decoded assets.</summary>
public sealed class Scene : NativeHandle
{
    /// <summary>Identifier of the scene root.</summary>
    public const uint Root = 0;

    /// <summary>Creates an empty scene framed to the given viewport.</summary>
    public Scene(float width, float height) : base(CreateHandle(width, height))
    {
    }

    private Scene(IntPtr handle) : base(handle)
    {
    }

    /// <summary>Wraps an existing handle; the wrapper takes ownership.</summary>
    public static Scene FromHandle(IntPtr handle) => new(handle);

    private static IntPtr CreateHandle(float width, float height)
    {
        MiuStatus status = Native.CreateScene(width, height, out IntPtr handle);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "scene create");
        }
        return handle;
    }

    /// <summary>Copies the scene's decoded assets into a renderer.</summary>
    public void AttachTo(Renderer renderer) => Check(Native.AttachScene(Handle, renderer.Handle), "scene attach");

    /// <summary>Sets the clear colour.</summary>
    public void SetBackground(Rgba8 color) => Check(Native.SetBackground(Handle, true, color), "scene background");

    /// <summary>Clears the background.</summary>
    public void ClearBackground() => Check(Native.SetBackground(Handle, false, default), "scene background");

    /// <summary>Replaces the camera.</summary>
    public void SetCamera(in Camera2D camera) => Check(Native.SetCamera(Handle, camera), "scene camera");

    /// <summary>Adds a group node.</summary>
    public uint AddGroup(uint parent, string name, Vec2 position = default)
    {
        Check(Native.AddGroup(Handle, parent, NameBytes(name), position, out uint id), "scene add group");
        return id;
    }

    /// <summary>Adds a sprite node.</summary>
    public uint AddSprite(uint parent, string name, ulong texture, Vec2 size, Vec2 pivot, Vec2 position,
                          Color color, BlendMode blend = BlendMode.SourceOver, int layer = 0)
    {
        Check(Native.AddSprite(Handle, parent, NameBytes(name), texture, size, pivot, position, color, blend,
                               layer, out uint id),
              "scene add sprite");
        return id;
    }

    /// <summary>Adds a text node.</summary>
    public uint AddText(uint parent, string name, ulong font, string text, TextStyle style, Vec2 position,
                        Color color, int layer = 0)
    {
        Check(Native.AddText(Handle, parent, NameBytes(name), font, Utf8.Bytes(text), style, color, position,
                             layer, out uint id),
              "scene add text");
        return id;
    }

    /// <summary>Adds a filled rectangle node.</summary>
    public uint AddRect(uint parent, string name, Rect rect, Color color, int layer = 0)
    {
        Check(Native.AddRect(Handle, parent, NameBytes(name), rect, color, layer, out uint id), "scene add rect");
        return id;
    }

    /// <summary>Replaces a node's transform.</summary>
    public void SetTransform(uint node, in Transform2D transform) =>
        Check(Native.SetTransform(Handle, node, transform), "scene transform");

    /// <summary>Shows or hides a node.</summary>
    public void SetVisible(uint node, bool visible) => Check(Native.SetVisible(Handle, node, visible), "scene visibility");

    /// <summary>Sets a node's opacity.</summary>
    public void SetOpacity(uint node, float opacity) => Check(Native.SetOpacity(Handle, node, opacity), "scene opacity");

    /// <summary>Sets a node's painter layer.</summary>
    public void SetLayer(uint node, int layer) => Check(Native.SetLayer(Handle, node, layer), "scene layer");

    /// <summary>Removes a node and its subtree.</summary>
    public void Remove(uint node) => Check(Native.RemoveNode(Handle, node), "scene remove");

    /// <summary>Node count including the root.</summary>
    public uint NodeCount()
    {
        Check(Native.NodeCount(Handle, out uint count), "scene node count");
        return count;
    }

    /// <summary>Number of animations.</summary>
    public uint AnimationCount()
    {
        Check(Native.AnimationCount(Handle, out uint count), "scene animation count");
        return count;
    }

    /// <summary>Applies every animation at an absolute time in seconds.</summary>
    public void Update(float timeSeconds) => Check(Native.UpdateScene(Handle, timeSeconds), "scene update");

    /// <summary>Finds the topmost node containing a world-space point.</summary>
    public bool HitTest(Vec2 world, out uint node)
    {
        MiuStatus status = Native.HitTest(Handle, world, out node);
        if (status == MiuStatus.NotFound)
        {
            return false;
        }
        Check(status, "scene hit test");
        return true;
    }

    /// <summary>Draws the scene with its own camera, background, and assets.</summary>
    public void Draw(Renderer renderer) => Check(Native.DrawScene(Handle, renderer.Handle), "scene draw");

    private static byte[]? NameBytes(string name) => string.IsNullOrEmpty(name) ? null : Utf8.NullTerminated(name);

    private static void Check(MiuStatus status, string action)
    {
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, action);
        }
    }

    /// <inheritdoc/>
    protected override void ReleaseNative(IntPtr handle) => Native.ReleaseScene(handle);
}

/// <summary>Declared asset in a scene document.</summary>
public sealed class DocumentAsset
{
    /// <summary>Creates a declared asset record.</summary>
    public DocumentAsset(string path, AssetKind kind)
    {
        Path = path;
        Kind = kind;
    }

    /// <summary>Path the host must resolve.</summary>
    public string Path { get; }

    /// <summary>Asset category.</summary>
    public AssetKind Kind { get; }
}

/// <summary>Scene document: JSON structure plus the host-supplied assets.</summary>
public sealed class Document : NativeHandle
{
    private Document(IntPtr handle) : base(handle)
    {
    }

    /// <summary>Parses a JSON document.</summary>
    public static Document Parse(string json)
    {
        MiuStatus status = Native.ParseDocument(Utf8.Bytes(json), out IntPtr handle);
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, "document parse");
        }
        return new Document(handle);
    }

    /// <summary>Parses a document file.</summary>
    public static Document LoadFile(string path) => Parse(File.ReadAllText(path));

    /// <summary>Authored viewport size.</summary>
    public Vec2 Viewport()
    {
        Check(Native.DocumentViewport(Handle, out Vec2 size), "document viewport");
        return size;
    }

    /// <summary>Declared assets, in document order.</summary>
    public IReadOnlyList<DocumentAsset> Assets()
    {
        Check(Native.DocumentAssetCount(Handle, out uint count), "document assets");
        var assets = new List<DocumentAsset>((int)count);
        for (uint index = 0; index < count; index++)
        {
            byte[] buffer = new byte[512];
            MiuStatus status = Native.DocumentAssetPath(Handle, index, buffer, out UIntPtr length);
            if (status != MiuStatus.Ok)
            {
                throw new MiuException(status, "document asset path");
            }
            string path = System.Text.Encoding.UTF8.GetString(buffer, 0, (int)length);
            Check(Native.DocumentAssetKind(Handle, index, out AssetKind kind), "document asset kind");
            assets.Add(new DocumentAsset(path, kind));
        }
        return assets;
    }

    /// <summary>Supplies a decoded texture for an asset path.</summary>
    public void SetTexture(string path, Texture texture) =>
        Check(Native.DocumentSetTexture(Handle, Utf8.NullTerminated(path), texture.Handle), "document texture");

    /// <summary>Supplies a parsed font for an asset path.</summary>
    public void SetFont(string path, Font font) =>
        Check(Native.DocumentSetFont(Handle, Utf8.NullTerminated(path), font.Handle), "document font");

    /// <summary>Builds the scene from the supplied assets.</summary>
    public Scene Instantiate()
    {
        Check(Native.InstantiateDocument(Handle, out IntPtr scene), "document instantiate");
        return Scene.FromHandle(scene);
    }

    private static void Check(MiuStatus status, string action)
    {
        if (status != MiuStatus.Ok)
        {
            throw new MiuException(status, action);
        }
    }

    /// <inheritdoc/>
    protected override void ReleaseNative(IntPtr handle) => Native.ReleaseDocument(handle);
}

/// <summary>Renderer plus camera and frame helpers.</summary>
public sealed class CpuRenderer : IDisposable
{
    /// <summary>Creates a renderer and a camera framed to its framebuffer.</summary>
    public CpuRenderer(uint width, uint height, Preset preset = Preset.High)
    {
        Width = width;
        Height = height;
        Renderer = new Renderer(width, height, preset);
        Camera = Camera2D.ForViewport(width, height);
    }

    /// <summary>Framebuffer width in pixels.</summary>
    public uint Width { get; private set; }

    /// <summary>Framebuffer height in pixels.</summary>
    public uint Height { get; private set; }

    /// <summary>Underlying renderer.</summary>
    public Renderer Renderer { get; }

    /// <summary>Camera used by the convenience draw calls.</summary>
    public Camera2D Camera { get; set; }

    /// <summary>Begins a frame and clears it.</summary>
    public void BeginFrame(Rgba8 clear) => Renderer.BeginFrame(clear);

    /// <summary>Fills a rectangle with the current camera.</summary>
    public void Fill(Rect rect, Color color) => Renderer.FillRect(Camera, rect, color);

    /// <summary>Draws a sprite with the current camera.</summary>
    public void Draw(in Sprite sprite) => Renderer.DrawSprite(Camera, sprite);

    /// <summary>Statistics for the frame.</summary>
    public RenderStats Stats() => Renderer.Stats();

    /// <summary>Encodes the frame as PNG.</summary>
    public byte[] EncodePng() => Renderer.EncodePng();

    /// <summary>Resizes the framebuffer and reframes the camera.</summary>
    public void Resize(uint width, uint height)
    {
        Renderer.Resize(width, height);
        Width = width;
        Height = height;
        Camera = Camera2D.ForViewport(width, height);
    }

    /// <inheritdoc/>
    public void Dispose() => Renderer.Dispose();
}
