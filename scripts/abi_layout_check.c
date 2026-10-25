/* Manual ABI-layout guard. Compile through scripts/verify-manually.sh. */
#include "miu/miu.h"

_Static_assert(sizeof(miu_render_plan) == 4, "ABI v1 render plan layout changed");
_Static_assert(offsetof(miu_render_plan, backend) == 0, "ABI v1 plan backend offset changed");
_Static_assert(offsetof(miu_render_plan, tier) == 1, "ABI v1 plan tier offset changed");
_Static_assert(offsetof(miu_render_plan, emergency_fallback) == 2, "ABI v1 plan fallback offset changed");
_Static_assert(offsetof(miu_render_plan, below_minimum) == 3, "ABI v1 plan minimum offset changed");

_Static_assert(sizeof(miu_render_plan_v2) == 5, "ABI v2 render plan layout changed");
_Static_assert(offsetof(miu_render_plan_v2, backend) == 0, "ABI v2 plan backend offset changed");
_Static_assert(offsetof(miu_render_plan_v2, profile_backend) == 1, "ABI v2 plan profile offset changed");
_Static_assert(offsetof(miu_render_plan_v2, tier) == 2, "ABI v2 plan tier offset changed");
_Static_assert(offsetof(miu_render_plan_v2, emergency_fallback) == 3, "ABI v2 plan fallback offset changed");
_Static_assert(offsetof(miu_render_plan_v2, below_minimum) == 4, "ABI v2 plan minimum offset changed");

_Static_assert(sizeof(miu_cpu_sprite) == 56, "ABI v1 CPU sprite layout changed");
_Static_assert(offsetof(miu_cpu_sprite, transform) == 0, "ABI v1 sprite transform offset changed");
_Static_assert(offsetof(miu_cpu_sprite, uv) == 20, "ABI v1 sprite UV offset changed");
_Static_assert(offsetof(miu_cpu_sprite, color) == 36, "ABI v1 sprite color offset changed");
_Static_assert(offsetof(miu_cpu_sprite, layer) == 52, "ABI v1 sprite layer offset changed");

_Static_assert(sizeof(miu_cpu_sprite_v2) == 88, "ABI v2 CPU sprite layout changed");
_Static_assert(offsetof(miu_cpu_sprite_v2, transform) == 0, "ABI v2 sprite transform offset changed");
_Static_assert(offsetof(miu_cpu_sprite_v2, source) == 20, "ABI v2 sprite source offset changed");
_Static_assert(offsetof(miu_cpu_sprite_v2, uv) == 36, "ABI v2 sprite UV offset changed");
_Static_assert(offsetof(miu_cpu_sprite_v2, size) == 52, "ABI v2 sprite size offset changed");
_Static_assert(offsetof(miu_cpu_sprite_v2, pivot) == 60, "ABI v2 sprite pivot offset changed");
_Static_assert(offsetof(miu_cpu_sprite_v2, color) == 68, "ABI v2 sprite color offset changed");
_Static_assert(offsetof(miu_cpu_sprite_v2, layer) == 84, "ABI v2 sprite layer offset changed");

int main(void) {
    return 0;
}
