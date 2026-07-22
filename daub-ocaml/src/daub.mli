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

(* file: winit.rs *)

type redraw_mode = On_demand | Continuous
type event_action = None | Redraw | Exit
