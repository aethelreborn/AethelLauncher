package dev.aethel.cosmetics.mixin;

import com.mojang.blaze3d.vertex.PoseStack;
import dev.aethel.cosmetics.CosmeticsState;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.layers.CapeLayer;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Hands vanilla our cape texture so it animates and lights the cape exactly as it should. */
@Mixin(CapeLayer.class)
public abstract class CapeLayerMixin {

    @Inject(
            method = "submit(Lcom/mojang/blaze3d/vertex/PoseStack;Lnet/minecraft/client/renderer/SubmitNodeCollector;ILnet/minecraft/client/renderer/entity/state/AvatarRenderState;FF)V",
            at = @At("HEAD"))
    private void aethel$applyCape(
            PoseStack poseStack,
            SubmitNodeCollector collector,
            int packedLight,
            AvatarRenderState state,
            float partialTick,
            float age,
            CallbackInfo ci) {
        CosmeticsState.applyCape(state);
    }
}