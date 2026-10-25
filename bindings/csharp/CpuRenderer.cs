// CPU reference-renderer binding for Miu's stable C ABI.

using System;
using System.Runtime.InteropServices;

namespace Miu
{
    /// <summary>Premultiplied RGBA8 pixel used by <see cref="CpuTexture"/> and <see cref="CpuCanvas"/>.</summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct CpuPixel
    {
        public byte R;
        public byte G;
        public byte B;
        public byte A;

        public CpuPixel(byte r, byte g, byte b, byte a)
        {
            R = r;
            G = g;
            B = b;
            A = a;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct CpuVector2
    {
        public float X;
        public float Y;

        public CpuVector2(float x, float y)
        {
            X = x;
            Y = y;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct CpuColor
    {
        public float R;
        public float G;
        public float B;
        public float A;

        public static CpuColor White
        {
            get { return new CpuColor { R = 1.0f, G = 1.0f, B = 1.0f, A = 1.0f }; }
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct CpuTransform2D
    {
        public CpuVector2 Translation;
        public float RotationRadians;
        public CpuVector2 Scale;

        public static CpuTransform2D Identity
        {
            get
            {
                return new CpuTransform2D
                {
                    Scale = new CpuVector2(1.0f, 1.0f),
                };
            }
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct CpuUvRect
    {
        public float Left;
        public float Top;
        public float Right;
        public float Bottom;

        public static CpuUvRect Full
        {
            get { return new CpuUvRect { Right = 1.0f, Bottom = 1.0f }; }
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct CpuSourceRect
    {
        public uint X;
        public uint Y;
        public uint Width;
        public uint Height;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct CpuCamera2D
    {
        public CpuVector2 Position;
        public CpuVector2 Viewport;
        public float Zoom;
        public float RotationRadians;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct CpuSprite
    {
        public CpuTransform2D Transform;
        public CpuSourceRect Source;
        public CpuUvRect Uv;
        public CpuVector2 Size;
        public CpuVector2 Pivot;
        public CpuColor Color;
        public int Layer;
    }

    /// <summary>Move-only native CPU texture handle backed by premultiplied RGBA8 bytes.</summary>
    public sealed class CpuTexture : IDisposable
    {
        private IntPtr handle;

        private CpuTexture(IntPtr handle)
        {
            this.handle = handle;
        }

        ~CpuTexture()
        {
            Dispose(false);
        }

        /// <summary>
        /// Creates a texture from exactly <c>width * height * 4</c> premultiplied RGBA8 bytes.
        /// </summary>
        public static CpuTexture Create(uint width, uint height, byte[] rgbaBytes, out Status status)
        {
            if (rgbaBytes == null)
            {
                status = Status.InvalidArgument;
                return null;
            }
            IntPtr nativeTexture;
            if (!CpuNative.HasV2Abi())
            {
                status = Status.Unsupported;
                return null;
            }
            status = CpuNative.TextureCreate(width, height, rgbaBytes, new UIntPtr((uint)rgbaBytes.Length), out nativeTexture);
            return status == Status.Ok ? new CpuTexture(nativeTexture) : null;
        }

        /// <summary>Creates a texture from exactly <c>width * height * 4</c> straight-alpha RGBA8 bytes.</summary>
        public static CpuTexture CreateStraightAlpha(uint width, uint height, byte[] rgbaBytes, out Status status)
        {
            if (rgbaBytes == null)
            {
                status = Status.InvalidArgument;
                return null;
            }
            if (!CpuNative.HasV2Abi())
            {
                status = Status.Unsupported;
                return null;
            }
            IntPtr nativeTexture;
            status = CpuNative.TextureCreateStraightAlpha(width, height, rgbaBytes, new UIntPtr((uint)rgbaBytes.Length), out nativeTexture);
            return status == Status.Ok ? new CpuTexture(nativeTexture) : null;
        }

        public bool IsValid
        {
            get { return handle != IntPtr.Zero; }
        }

        public void Dispose()
        {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        private void Dispose(bool disposing)
        {
            var released = handle;
            handle = IntPtr.Zero;
            if (released != IntPtr.Zero)
            {
                CpuNative.TextureRelease(released);
            }
        }

        internal IntPtr Handle
        {
            get { return handle; }
        }
    }

    /// <summary>Deterministic headless CPU canvas for tests, previews, and tools.</summary>
    public sealed class CpuCanvas : IDisposable
    {
        private IntPtr handle;

        private CpuCanvas(IntPtr handle, uint width, uint height)
        {
            this.handle = handle;
            Width = width;
            Height = height;
        }

        ~CpuCanvas()
        {
            Dispose(false);
        }

        public uint Width { get; private set; }
        public uint Height { get; private set; }

        public static CpuCanvas Create(uint width, uint height, out Status status)
        {
            if (!CpuNative.HasV2Abi())
            {
                status = Status.Unsupported;
                return null;
            }
            IntPtr nativeCanvas;
            status = CpuNative.CanvasCreate(width, height, out nativeCanvas);
            return status == Status.Ok ? new CpuCanvas(nativeCanvas, width, height) : null;
        }

        public Status Clear(CpuPixel color)
        {
            return handle == IntPtr.Zero ? Status.InvalidArgument : CpuNative.CanvasClear(handle, color);
        }

        public Status Draw(CpuCamera2D camera, CpuSprite sprite, CpuTexture texture)
        {
            if (handle == IntPtr.Zero || texture == null || !texture.IsValid)
            {
                return Status.InvalidArgument;
            }
            if (!CpuNative.HasV2Abi())
            {
                return Status.Unsupported;
            }
            return CpuNative.CanvasDraw(handle, ref camera, ref sprite, texture.Handle);
        }

        /// <summary>Copies a row-major premultiplied RGBA8 snapshot of the canvas.</summary>
        public Status CopyRgba8(out byte[] pixels)
        {
            pixels = null;
            if (handle == IntPtr.Zero)
            {
                return Status.InvalidArgument;
            }
            var byteCount = (ulong)Width * Height * 4UL;
            if (byteCount > int.MaxValue)
            {
                return Status.OutOfMemory;
            }
            byte[] snapshot;
            try
            {
                snapshot = new byte[(int)byteCount];
            }
            catch (OutOfMemoryException)
            {
                return Status.OutOfMemory;
            }
            var status = CpuNative.CanvasCopy(handle, snapshot, new UIntPtr(byteCount));
            if (status == Status.Ok)
            {
                pixels = snapshot;
            }
            return status;
        }

        public void Dispose()
        {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        private void Dispose(bool disposing)
        {
            var released = handle;
            handle = IntPtr.Zero;
            Width = 0;
            Height = 0;
            if (released != IntPtr.Zero)
            {
                CpuNative.CanvasRelease(released);
            }
        }
    }

    internal static class CpuNative
    {
        private const string Library = "miu_ffi";
        private const uint AbiVersion = 2;

        internal static bool HasV2Abi()
        {
            try
            {
                return Marshal.SizeOf(typeof(CpuSprite)) == 88
                    && Marshal.OffsetOf(typeof(CpuSprite), "Source").ToInt32() == 20
                    && Marshal.OffsetOf(typeof(CpuSprite), "Uv").ToInt32() == 36
                    && Marshal.OffsetOf(typeof(CpuSprite), "Size").ToInt32() == 52
                    && Marshal.OffsetOf(typeof(CpuSprite), "Pivot").ToInt32() == 60
                    && Marshal.OffsetOf(typeof(CpuSprite), "Color").ToInt32() == 68
                    && Marshal.OffsetOf(typeof(CpuSprite), "Layer").ToInt32() == 84
                    && AbiMinSupportedVersion() <= AbiVersion
                    && AbiVersionQuery() >= AbiVersion;
            }
            catch (EntryPointNotFoundException)
            {
                return false;
            }
        }

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_abi_version")]
        private static extern uint AbiVersionQuery();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_abi_min_supported_version")]
        private static extern uint AbiMinSupportedVersion();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_cpu_texture_create_rgba8")]
        internal static extern Status TextureCreate(
            uint width,
            uint height,
            [In] byte[] rgbaBytes,
            UIntPtr rgbaByteCount,
            out IntPtr texture);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_cpu_texture_create_straight_rgba8")]
        internal static extern Status TextureCreateStraightAlpha(
            uint width,
            uint height,
            [In] byte[] rgbaBytes,
            UIntPtr rgbaByteCount,
            out IntPtr texture);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_cpu_texture_release")]
        internal static extern void TextureRelease(IntPtr texture);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_cpu_canvas_create")]
        internal static extern Status CanvasCreate(uint width, uint height, out IntPtr canvas);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_cpu_canvas_release")]
        internal static extern void CanvasRelease(IntPtr canvas);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_cpu_canvas_clear")]
        internal static extern Status CanvasClear(IntPtr canvas, CpuPixel color);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_cpu_canvas_draw_sprite_v2")]
        internal static extern Status CanvasDraw(
            IntPtr canvas,
            ref CpuCamera2D camera,
            ref CpuSprite sprite,
            IntPtr texture);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_cpu_canvas_copy_rgba8")]
        internal static extern Status CanvasCopy(IntPtr canvas, [Out] byte[] rgbaBytes, UIntPtr rgbaByteCount);
    }
}
