/**
 * How many surfaces — dialogs, popovers, menus, the palette — are open, as
 * a count with a floor (ide/18): a native layer drawn over the page (the
 * browser tab's webview) hides while any is, so a dialog is never painted
 * under it. Entering and leaving are the surfaces' own effects; a leave
 * never takes the count below nought.
 */

/** @param {number} count @param {number} delta */
export function surfaces(count, delta) {
  return Math.max(0, (Number(count) || 0) + (Number(delta) || 0));
}

/** Whether a native layer may show: no surface is open. @param {number} count */
export function layerMayShow(count) {
  return surfaces(count, 0) === 0;
}
