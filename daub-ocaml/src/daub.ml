(* OCaml types shared with the daub-ocaml Rust bindings. *)

(* file: color.rs *)

module Color = struct
  type t = { red : float; green : float; blue : float; alpha : float }

  let rgba red green blue alpha = { red; green; blue; alpha }
  let rgb red green blue = rgba red green blue 1.
  let transparent = rgba 0. 0. 0. 0.
  let black = rgb 0. 0. 0.
  let white = rgb 1. 1. 1.
end

(* file: geometry.rs *)

module Layout_value = struct
  type t = Relative of float | Logical_pixels of float | Physical_pixels of float

  let relative value = Relative value
  let pixels value = Logical_pixels value
  let physical_pixels value = Physical_pixels value
  let zero = pixels 0.
end

module Point = struct
  type t = { x : Layout_value.t; y : Layout_value.t }

  let make x y = { x; y }
  let origin = make Layout_value.zero Layout_value.zero
end

module Size = struct
  type t = { width : Layout_value.t; height : Layout_value.t }

  let make width height = { width; height }
  let zero = make Layout_value.zero Layout_value.zero
end

module Anchor = struct
  type t = { x : float; y : float }

  let make x y = { x; y }
  let top_left = make 0. 0.
  let top = make 0.5 0.
  let top_right = make 1. 0.
  let left = make 0. 0.5
  let center = make 0.5 0.5
  let right = make 1. 0.5
  let bottom_left = make 0. 1.
  let bottom = make 0.5 1.
  let bottom_right = make 1. 1.
end

module Rectangle = struct
  type t = { position : Point.t; size : Size.t; anchor : Anchor.t }

  let make ?(anchor = Anchor.top_left) position size = { position; size; anchor }
  let from_anchor position size anchor = make ~anchor position size
  let from_center center size = make ~anchor:Anchor.center center size
  let empty = make Point.origin Size.zero
end

(* file: primitive/quad.rs *)

module Border = struct
  type t = { color : Color.t; width : Layout_value.t }

  let make color width = { color; width }
  let none = make Color.transparent Layout_value.zero
end

module Corner_radii = struct
  type t = {
    top_left : Layout_value.t;
    top_right : Layout_value.t;
    bottom_right : Layout_value.t;
    bottom_left : Layout_value.t;
  }

  let make top_left top_right bottom_right bottom_left =
    { top_left; top_right; bottom_right; bottom_left }

  let uniform radius = make radius radius radius radius
  let zero = uniform Layout_value.zero
end

module Quad = struct
  type t = {
    rectangle : Rectangle.t;
    color : Color.t;
    border : Border.t;
    corner_radii : Corner_radii.t;
  }

  let make ?(border = Border.none) ?(corner_radii = Corner_radii.zero) rectangle color =
    { rectangle; color; border; corner_radii }
end

(* file: primitive/text.rs *)

module Font_family = struct
  type t = Name of string | Serif | Sans_serif | Cursive | Fantasy | Monospace
end

module Font_weight = struct
  type t = { value : int }

  let make value = { value }
  let thin = make 100
  let extra_light = make 200
  let light = make 300
  let normal = make 400
  let medium = make 500
  let semibold = make 600
  let bold = make 700
  let extra_bold = make 800
  let black = make 900
end

module Font_stretch = struct
  type t =
    | Ultra_condensed
    | Extra_condensed
    | Condensed
    | Semi_condensed
    | Normal
    | Semi_expanded
    | Expanded
    | Extra_expanded
    | Ultra_expanded
end

module Font_style = struct
  type t = Normal | Italic | Oblique
end

module Text_wrap = struct
  type t = None | Glyph | Word | Word_or_glyph
end

module Shaping = struct
  type t = Basic | Advanced
end

module Text = struct
  type t = {
    rectangle : Rectangle.t;
    content : string;
    color : Color.t;
    font_size : float;
    line_height : float;
    family : Font_family.t;
    weight : Font_weight.t;
    stretch : Font_stretch.t;
    style : Font_style.t;
    wrap : Text_wrap.t;
    shaping : Shaping.t;
  }

  let make ?(font_size = 16.) ?(line_height = 20.) ?(family = Font_family.Sans_serif)
      ?(weight = Font_weight.normal) ?(stretch = Font_stretch.Normal)
      ?(style = Font_style.Normal) ?(wrap = Text_wrap.Word_or_glyph) ?(shaping = Shaping.Advanced)
      rectangle content color =
    {
      rectangle;
      content;
      color;
      font_size;
      line_height;
      family;
      weight;
      stretch;
      style;
      wrap;
      shaping;
    }
end

(* file: winit.rs *)

module Redraw_mode = struct
  type t = On_demand | Continuous
end

module Event_action = struct
  type t = None | Redraw | Exit
end
