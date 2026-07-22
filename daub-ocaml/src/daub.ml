(* OCaml types shared with the daub-ocaml Rust bindings. *)

(* file: color.rs *)

type color = { red : float; green : float; blue : float; alpha : float }

(* file: geometry.rs *)

type layout_value = Relative of float | Logical_pixels of float | Physical_pixels of float
type point = { x : layout_value; y : layout_value }
type size = { width : layout_value; height : layout_value }
type anchor = { x : float; y : float }
type rectangle = { position : point; size : size; anchor : anchor }

(* file: primitive/quad.rs *)

type border = { color : color; width : layout_value }
type corner_radii = {
  top_left : layout_value;
  top_right : layout_value;
  bottom_right : layout_value;
  bottom_left : layout_value;
}
type quad = { rectangle : rectangle; color : color; border : border; corner_radii : corner_radii }

(* file: primitive/text.rs *)

type font_family = Name of string | Serif | Sans_serif | Cursive | Fantasy | Monospace
type font_weight = { value : int }
type font_stretch =
  | Ultra_condensed
  | Extra_condensed
  | Condensed
  | Semi_condensed
  | Normal_stretch
  | Semi_expanded
  | Expanded
  | Extra_expanded
  | Ultra_expanded
type font_style = Normal_style | Italic | Oblique
type text_wrap = No_wrap | Glyph | Word | Word_or_glyph
type shaping = Basic | Advanced
type text = {
  rectangle : rectangle;
  content : string;
  color : color;
  font_size : float;
  line_height : float;
  family : font_family;
  weight : font_weight;
  stretch : font_stretch;
  style : font_style;
  wrap : text_wrap;
  shaping : shaping;
}

(* file: winit.rs *)

type redraw_mode = On_demand | Continuous
type event_action = None | Redraw | Exit
