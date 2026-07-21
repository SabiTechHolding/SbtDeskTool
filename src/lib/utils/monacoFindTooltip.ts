// Monaco 0.55 uses two hover implementations inside the find widget:
// - input toggles use setupManagedHover() and receive custom-hover="true";
// - Previous/Next/Close use setupDelayedHover() and only expose role/aria-label.
// Cover both DOM shapes so the body-level portal actually replaces every
// Monaco find tooltip.
const FIND_CONTROL_SELECTOR = [
  '.find-widget [custom-hover="true"]',
  '.find-widget [role="button"][aria-label]',
  '.find-widget [role="checkbox"][aria-label]',
].join(", ");

/**
 * Monaco's managed find-button hovers are rendered inside its layout context,
 * where parent stacking contexts can cover or clip them. Render the same label
 * in a fixed document-body portal so it is always above application controls.
 */
export function installFindTooltip(container: HTMLElement, controlSelector = FIND_CONTROL_SELECTOR) {
  let tooltip: HTMLDivElement | null = null;
  let activeControl: HTMLElement | null = null;
  const tabPanel = container.closest<HTMLElement>(".tab-panel");

  function removeTooltip() {
    tooltip?.remove();
    tooltip = null;
    activeControl = null;
  }

  function removeOtherTooltips() {
    document.querySelectorAll(".sbt-monaco-find-tooltip").forEach((element) => {
      if (element !== tooltip) element.remove();
    });
  }

  function findControl(target: EventTarget | null) {
    return target instanceof Element
      ? target.closest<HTMLElement>(controlSelector)
      : null;
  }

  function isFindInputTarget(element: Element | null) {
    if (!element) return false;
    if (element.closest(".find-widget textarea, .find-widget input")) return true;
    // Depending on the exact pixel under the pointer, Monaco may report the
    // inputbox wrapper instead of its nested input as the mouseover target.
    const hoverOwner = element.closest<HTMLElement>('.find-widget [custom-hover="true"]');
    return Boolean(hoverOwner?.querySelector("textarea, input"));
  }

  function showTooltip(control: HTMLElement) {
    const label = control.getAttribute("aria-label")?.trim();
    if (!label) return;
    const controlRect = control.getBoundingClientRect();
    // A hidden tab can retain Monaco's `visible` state while its DOM has no
    // layout box. Such a control cannot be hovered by a user and must not
    // create a stale tooltip at the viewport origin.
    if (controlRect.width <= 0 || controlRect.height <= 0) return;
    if (activeControl === control && tooltip?.isConnected) return;

    removeTooltip();
    // Every editor and common FindBar has its own installer. Keep the portal
    // global-singleton so moving between tabs/editors can never leave two
    // overlapping tooltip nodes behind.
    removeOtherTooltips();
    control.removeAttribute("title");
    activeControl = control;
    tooltip = document.createElement("div");
    tooltip.className = "sbt-monaco-find-tooltip";
    tooltip.textContent = label;
    Object.assign(tooltip.style, {
      position: "fixed",
      zIndex: "2147483647",
      pointerEvents: "none",
      padding: "5px 8px",
      border: "1px solid #454545",
      borderRadius: "4px",
      background: "var(--bg3, #252526)",
      color: "var(--fg, #f0f0f0)",
      boxShadow: "0 3px 10px rgba(0, 0, 0, 0.45)",
      font: "12px/18px system-ui, sans-serif",
      whiteSpace: "nowrap",
    });
    document.body.appendChild(tooltip);

    const tooltipRect = tooltip.getBoundingClientRect();
    const gap = 6;
    // All find-control tooltips are intentionally placed below their control.
    // Monaco's checkbox toggles normally open upward, where application bars
    // cover them; using one direction keeps every control consistent.
    const top = controlRect.bottom + gap;
    const left = Math.min(
      Math.max(4, controlRect.left + controlRect.width / 2 - tooltipRect.width / 2),
      Math.max(4, window.innerWidth - tooltipRect.width - 4),
    );
    tooltip.style.top = `${Math.round(top)}px`;
    tooltip.style.left = `${Math.round(left)}px`;
  }

  function handleMouseOver(event: MouseEvent) {
    const eventElement = event.target instanceof Element ? event.target : null;
    // Monaco attaches managed-hover metadata to the find input wrapper too.
    // A text caret does not need a tooltip; entering the input must also clear
    // a tooltip left behind by the adjacent option buttons.
    if (isFindInputTarget(eventElement)) {
      // Prevent Monaco's managed-hover listener from opening its own upward
      // tooltip after the body-level tooltip has been cleared.
      event.stopPropagation();
      removeTooltip();
      return;
    }
    const control = findControl(event.target);
    if (!control || !container.contains(control)) return;
    // Stop Monaco's managed hover before its target-level capture listener.
    event.stopPropagation();
    showTooltip(control);
  }

  function handleMouseOut(event: MouseEvent) {
    if (!activeControl) return;
    const related = event.relatedTarget;
    if (related instanceof Node && activeControl.contains(related)) return;
    removeTooltip();
  }

  function handleFocus(event: FocusEvent) {
    const eventElement = event.target instanceof Element ? event.target : null;
    if (isFindInputTarget(eventElement)) {
      event.stopPropagation();
      removeTooltip();
      return;
    }
    const control = findControl(event.target);
    if (!control || !container.contains(control)) return;
    event.stopPropagation();
    showTooltip(control);
  }

  function handleBlur() {
    removeTooltip();
  }

  container.addEventListener("mouseover", handleMouseOver, true);
  container.addEventListener("mouseout", handleMouseOut, true);
  container.addEventListener("focus", handleFocus, true);
  container.addEventListener("blur", handleBlur, true);
  window.addEventListener("resize", removeTooltip);
  window.addEventListener("scroll", removeTooltip, true);
  const tabObserver = tabPanel
    ? new MutationObserver(() => {
        if (!tabPanel.classList.contains("active")) removeTooltip();
      })
    : null;
  tabObserver?.observe(tabPanel!, { attributes: true, attributeFilter: ["class"] });

  return () => {
    removeTooltip();
    tabObserver?.disconnect();
    container.removeEventListener("mouseover", handleMouseOver, true);
    container.removeEventListener("mouseout", handleMouseOut, true);
    container.removeEventListener("focus", handleFocus, true);
    container.removeEventListener("blur", handleBlur, true);
    window.removeEventListener("resize", removeTooltip);
    window.removeEventListener("scroll", removeTooltip, true);
  };
}

export function installMonacoFindTooltip(container: HTMLElement) {
  return installFindTooltip(container);
}
