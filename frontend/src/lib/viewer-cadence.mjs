/** Keep fractional frame time so refresh-rate rounding cannot lower the draw cap. */
export function advanceRenderClock(now,previous,interval){
  // Timestamps rounded to tenths of a millisecond may arrive just before a deadline.
  // Keep the deadline anchored; a delayed refresh paints once and drops missed paints.
  const ticks=Math.floor((now-previous+.1)/interval);
  return ticks<1?null:previous+ticks*interval;
}
