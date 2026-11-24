<script>
  export let title = "Nothing here yet";
  export let description = "";
  export let actionLabel = "";
  export let onAction = null;
</script>

<div class="empty-state-box">
  <div class="chibi-wrap">
    <div class="ambient-glow"></div>
    <img src="/chibi-empty.png" alt="ZibraX Chibi" class="chibi-art" />
  </div>
  <h3 class="empty-title">{title}</h3>
  {#if description}
    <p class="empty-desc">{description}</p>
  {/if}
  {#if actionLabel && onAction}
    <button class="empty-action-btn" on:click={onAction}>
      {actionLabel}
    </button>
  {/if}
</div>

<style>
  .empty-state-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 420px;
    padding: var(--z-space-8) var(--z-space-4);
    text-align: center;
    margin: 0 auto;
    max-width: 460px;
  }

  .chibi-wrap {
    position: relative;
    width: 200px;
    height: 200px;
    margin-bottom: var(--z-space-5);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .ambient-glow {
    position: absolute;
    width: 160px;
    height: 160px;
    background: radial-gradient(circle, var(--z-accent-glow) 0%, var(--z-accent-muted) 50%, transparent 70%);
    border-radius: 50%;
    filter: blur(20px);
    z-index: 1;
  }

  .chibi-art {
    width: 100%;
    height: 100%;
    object-fit: contain;
    position: relative;
    z-index: 2;
    animation: chibiFloat 4.5s ease-in-out infinite;
    filter: drop-shadow(0 12px 24px rgba(0, 0, 0, 0.6));
  }

  @keyframes chibiFloat {
    0%, 100% {
      transform: translateY(0px) rotate(0deg);
    }
    50% {
      transform: translateY(-8px) rotate(0.8deg);
    }
  }

  .empty-title {
    font-size: var(--z-text-xl);
    font-weight: 700;
    color: var(--z-text);
    margin-bottom: var(--z-space-2);
    letter-spacing: -0.3px;
  }

  .empty-desc {
    font-size: var(--z-text-sm);
    color: var(--z-text-muted);
    line-height: 1.55;
    margin-bottom: var(--z-space-6);
  }

  .empty-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 22px;
    background: var(--z-accent);
    color: #ffffff;
    font-size: var(--z-text-sm);
    font-weight: 600;
    border-radius: var(--z-radius-full);
    border: none;
    cursor: pointer;
    box-shadow: 0 4px 16px var(--z-accent-glow);
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .empty-action-btn:hover {
    filter: brightness(1.08);
    transform: translateY(-2px);
    box-shadow: 0 8px 24px var(--z-accent-glow);
  }
</style>
