package dev.aethel.cosmetics.mixin;

import dev.aethel.cosmetics.CosmeticsState;
import net.minecraft.client.renderer.entity.layers.WingsLayer;
import net.minecraft.client.renderer.entity.state.HumanoidRenderState;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Swaps the wings texture when the player has an equipped elytra or wings cosmetic. */
@Mixin(WingsLayer.class)
public abstract class WingsLayerMixin {

    @Inject(method = "getPlayerElytraTexture", at = @At("HEAD"), cancellable = true)
    private void aethel$applyWings(
            HumanoidRenderState state, CallbackInfoReturnable<Identifier> cir) {
        Identifier ours = CosmeticsState.wingsIdentifier();
        if (ours != null) {
            cir.setReturnValue(ours);
        }
    }
}