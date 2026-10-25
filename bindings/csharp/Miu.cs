// Miu C# binding. Targets netstandard2.0 and has no NuGet dependency.
// Native library name: miu_ffi (miu_ffi.dll / libmiu_ffi.so / libmiu_ffi.dylib).

using System;
using System.Runtime.InteropServices;

namespace Miu
{
    public enum Platform : byte
    {
        Windows = 1,
        Android = 2,
        MacOS = 3,
        IOS = 4,
        Linux = 5,
        Web = 6,
    }

    public enum Backend : byte
    {
        Direct2D12 = 1,
        Direct3D113 = 2,
        Direct3D12 = 3,
        OpenGLES30 = 4,
        OpenGLES20 = 5,
        Metal2 = 6,
        Metal3 = 7,
        Metal1 = 8,
        Vulkan11 = 9,
        Vulkan12 = 10,
        Vulkan13 = 11,
        Vulkan14 = 12,
        OpenGL33 = 13,
        OpenGL30 = 14,
        WebGL2 = 15,
        WebGL1 = 16,
        WebGPU = 17,
    }

    public enum RenderTier : byte
    {
        Compatibility = 1,
        Balanced = 2,
        Premium = 3,
    }

    public enum Status : uint
    {
        Ok = 0,
        InvalidArgument = 1,
        Unsupported = 2,
        OutOfMemory = 3,
    }

    /// <summary>Requested project minimum and preferred rendering tier.</summary>
    public struct RendererRequest
    {
        public RenderTier MinimumTier;
        /// <summary>Preferred compliant tier; stronger tiers are tried only when this cannot run.</summary>
        public RenderTier PreferredTier;

        /// <summary>
        /// Explicitly permits a below-minimum or emergency fallback. The caller must
        /// warn the end user if the returned plan requires it.
        /// </summary>
        public bool AllowBestEffort;

        public RendererRequest(RenderTier minimumTier)
        {
            MinimumTier = minimumTier;
            PreferredTier = minimumTier;
            AllowBestEffort = false;
        }
    }

    /// <summary>Immutable API selection made by the native policy core.</summary>
    public struct RenderPlan
    {
        /// <summary>The actual API reported by the selected device.</summary>
        public Backend Backend { get; internal set; }
        /// <summary>The minimum catalog API profile satisfied by <see cref="Backend"/>.</summary>
        public Backend ProfileBackend { get; internal set; }
        public RenderTier Tier { get; internal set; }
        public bool EmergencyFallback { get; internal set; }
        public bool BelowMinimum { get; internal set; }

        public bool RequiresForceRunWarning
        {
            get { return EmergencyFallback || BelowMinimum; }
        }
    }

    /// <summary>
    /// An owning policy-renderer handle. It selects a backend; an application-owned
    /// platform bridge creates the actual graphics device for <see cref="RenderPlan.Backend"/>.
    /// </summary>
    public sealed class Renderer : IDisposable
    {
        private IntPtr handle;

        private Renderer(IntPtr handle)
        {
            this.handle = handle;
        }

        ~Renderer()
        {
            Dispose(false);
        }

        /// <summary>
        /// Creates a renderer policy handle from APIs that have already passed the
        /// application's platform probe. The input array is copied before this call returns.
        /// A null result has the reason in <paramref name="status"/>.
        /// </summary>
        public static Renderer Create(
            Platform platform,
            Backend[] availableBackends,
            RendererRequest request,
            out Status status)
        {
            if (availableBackends == null)
            {
                status = Status.InvalidArgument;
                return null;
            }
            if (!Native.HasV2Abi())
            {
                status = Status.Unsupported;
                return null;
            }

            IntPtr backendMemory = IntPtr.Zero;
            try
            {
                if (availableBackends.Length != 0)
                {
                    backendMemory = Marshal.AllocHGlobal(availableBackends.Length);
                    for (var index = 0; index < availableBackends.Length; index++)
                    {
                        Marshal.WriteByte(backendMemory, index, (byte)availableBackends[index]);
                    }
                }

                var capabilities = new NativeCapabilities
                {
                    AbiVersion = Native.AbiVersion,
                    Platform = (byte)platform,
                    Backends = backendMemory,
                    BackendCount = new UIntPtr((uint)availableBackends.Length),
                };
                var nativeRequest = new NativeRequest
                {
                    AbiVersion = Native.AbiVersion,
                    MinimumTier = (byte)request.MinimumTier,
                    PreferredTier = (byte)request.PreferredTier,
                    AllowBestEffort = request.AllowBestEffort ? (byte)1 : (byte)0,
                };
                IntPtr nativeRenderer;
                status = Native.RendererCreate(ref capabilities, ref nativeRequest, out nativeRenderer);
                return status == Status.Ok ? new Renderer(nativeRenderer) : null;
            }
            finally
            {
                if (backendMemory != IntPtr.Zero)
                {
                    Marshal.FreeHGlobal(backendMemory);
                }
            }
        }

        /// <summary>Gets the selected plan from a live handle.</summary>
        public Status GetPlan(out RenderPlan plan)
        {
            plan = default(RenderPlan);
            if (handle == IntPtr.Zero)
            {
                return Status.InvalidArgument;
            }
            if (!Native.HasV2Abi())
            {
                return Status.Unsupported;
            }
            NativePlan nativePlan;
            var status = Native.RendererGetPlan(handle, out nativePlan);
            if (status == Status.Ok)
            {
                plan.Backend = (Backend)nativePlan.Backend;
                plan.ProfileBackend = (Backend)nativePlan.ProfileBackend;
                plan.Tier = (RenderTier)nativePlan.Tier;
                plan.EmergencyFallback = nativePlan.EmergencyFallback != 0;
                plan.BelowMinimum = nativePlan.BelowMinimum != 0;
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
            if (released != IntPtr.Zero)
            {
                Native.RendererRelease(released);
            }
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct NativeCapabilities
        {
            internal uint AbiVersion;
            internal byte Platform;
            internal byte Reserved0;
            internal byte Reserved1;
            internal byte Reserved2;
            internal IntPtr Backends;
            internal UIntPtr BackendCount;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct NativeRequest
        {
            internal uint AbiVersion;
            internal byte MinimumTier;
            internal byte PreferredTier;
            internal byte AllowBestEffort;
            internal byte Reserved;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct NativePlan
        {
            internal byte Backend;
            internal byte ProfileBackend;
            internal byte Tier;
            internal byte EmergencyFallback;
            internal byte BelowMinimum;
        }

        private static class Native
        {
            internal const uint AbiVersion = 2;
            private const string Library = "miu_ffi";

            internal static bool HasV2Abi()
            {
                try
                {
                    return Marshal.SizeOf(typeof(NativePlan)) == 5
                        && Marshal.OffsetOf(typeof(NativePlan), "ProfileBackend").ToInt32() == 1
                        && Marshal.OffsetOf(typeof(NativePlan), "BelowMinimum").ToInt32() == 4
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

            [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_renderer_create")]
            internal static extern Status RendererCreate(
                ref NativeCapabilities capabilities,
                ref NativeRequest request,
                out IntPtr renderer);

            [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_renderer_get_plan_v2")]
            internal static extern Status RendererGetPlan(IntPtr renderer, out NativePlan plan);

            [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "miu_renderer_release")]
            internal static extern void RendererRelease(IntPtr renderer);
        }
    }
}
