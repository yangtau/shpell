# shpell — natural language to shell commands (zsh integration)
#
# Press Tab on an empty line to enter Shpell mode: an interactive prompt run by
# `shpell compose`, entirely outside zle. Type a request after the ❯ prompt;
# the generated command streams in after the ✻ icon, which pulses while
# generating. Then:
#   Enter (empty)   accept — back to zsh with the command on the prompt,
#                   NOT run; you decide whether to run, edit or discard it.
#                   The whole exchange stays on screen above the prompt
#   more text       refine the command with a follow-up request
#   Esc / Ctrl-C / Ctrl-D cancel
#
# Because input and streaming happen inside `shpell compose`, zle never sees
# the natural-language text — no syntax-highlighting, history-expansion or PS2
# surprises. Icons: export SHPELL_USER_ICON / SHPELL_AI_ICON before use to
# override.

(( ${+_SHPELL_ZSH_LOADED} )) && return
typeset -g _SHPELL_ZSH_LOADED=1

# the model may answer non-command requests with a `# ...` comment line
setopt interactive_comments

typeset -g _shpell_tab_fallback=  # widget Tab was bound to before we took it over

_shpell_compose() {
  # only hijack Tab on an empty primary prompt; otherwise delegate to completion
  if [[ -n $BUFFER || $CONTEXT != start ]]; then
    zle "${_shpell_tab_fallback:-expand-or-complete}"
    return
  fi
  local out rc=1 ttysave
  zle -I   # shpell compose takes over drawing from here
  ttysave=$(stty -g < /dev/tty 2>/dev/null)
  {
    # zle keeps the tty raw during widgets; give shpell compose a cooked tty
    # so the terminal driver provides line editing and echo for the query
    stty sane < /dev/tty 2>/dev/null
    out=$(COLUMNS=$COLUMNS command shpell compose --shell zsh < /dev/tty 2> /dev/tty)
    rc=$?
  } always {
    [[ -n $ttysave ]] && stty "$ttysave" < /dev/tty 2>/dev/null
  }
  case $rc in
    0)   # accepted: leave the command on the prompt; the user runs it
      BUFFER=$out
      CURSOR=$#BUFFER
      zle reset-prompt
      ;;
    *)   zle reset-prompt ;;
  esac
}
zle -N _shpell_compose

# Claim Tab from precmd so we run after every other plugin (fzf-tab etc.) has
# bound it; remember the previous widget so non-empty lines still complete.
_shpell_install() {
  local km cur
  for km in main viins emacs; do
    cur=${${(z)$(bindkey -M $km '^I' 2>/dev/null)}[2]}
    [[ -n $cur && $cur != _shpell_compose ]] && _shpell_tab_fallback=$cur
    bindkey -M $km '^I' _shpell_compose 2>/dev/null
  done
  add-zsh-hook -d precmd _shpell_install
}
autoload -Uz add-zsh-hook
add-zsh-hook precmd _shpell_install
