# shpell — natural language to shell commands (bash integration)
#
# Press Tab on an empty line to enter Shpell mode: an interactive prompt run by
# `shpell compose`, entirely outside readline. Type a request after the ❯
# prompt; the generated command streams in after the ✻ icon, which pulses
# while generating. Then:
#   Enter (empty)   accept — back to bash with the command on the prompt,
#                   NOT run; you decide whether to run, edit or discard it.
#                   The whole exchange stays on screen above the prompt
#   more text       refine the command with a follow-up request
#   Ctrl-C / Ctrl-D cancel
#
# bash cannot dispatch conditionally inside readline, so Tab is bound to a
# two-key macro: the first key runs a `bind -x` handler which, on a non-empty
# line, rebinds the second key to the original completion function and, on an
# empty line, runs `shpell compose` and rebinds the second key to a no-op.
# The final `\e[5n` device-status query makes the terminal answer `\e[0n`,
# which is bound to redraw-current-line so the prompt is redrawn wherever
# `shpell compose` left the cursor.

if [[ $- == *i* && -z ${_SHPELL_BASH_LOADED-} ]] && ((BASH_VERSINFO[0] >= 4)); then
_SHPELL_BASH_LOADED=1

# the model may answer non-command requests with a `# ...` comment line
shopt -s interactive_comments 2>/dev/null

# completion function Tab was bound to before we took it over
_shpell_tab_fallback=$(bind -p 2>/dev/null | command sed -n 's/^"\\C-i": \([a-z-]\{1,\}\)$/\1/p')
[[ -z $_shpell_tab_fallback ]] && _shpell_tab_fallback=complete

_shpell_compose() {
  if [[ -n $READLINE_LINE ]]; then
    # non-empty line: hand the second macro key to the original completion
    bind '"\C-x\C-y2": '"$_shpell_tab_fallback"
    return
  fi
  bind '"\C-x\C-y2": ""'  # empty line: swallow the second macro key
  local out rc=1 ttysave
  ttysave=$(stty -g < /dev/tty 2>/dev/null)
  # give shpell compose a cooked tty so the terminal driver provides line
  # editing and echo for the query
  stty sane < /dev/tty 2>/dev/null
  out=$(COLUMNS=$COLUMNS command shpell compose --shell bash < /dev/tty 2> /dev/tty)
  rc=$?
  [[ -n $ttysave ]] && stty "$ttysave" < /dev/tty 2>/dev/null
  if ((rc == 0)); then
    # accepted: leave the command on the prompt; the user runs it
    READLINE_LINE=$out
    READLINE_POINT=${#READLINE_LINE}
  fi
  printf '\e[5n' > /dev/tty  # terminal replies \e[0n → redraw-current-line
}

for _shpell_km in emacs vi-insert; do
  bind -m $_shpell_km -x '"\C-x\C-y1": _shpell_compose'
  bind -m $_shpell_km '"\C-x\C-y2": complete'
  bind -m $_shpell_km '"\e[0n": redraw-current-line'
  bind -m $_shpell_km '"\C-i": "\C-x\C-y1\C-x\C-y2"'
done
unset _shpell_km

fi
