### Bisa — the desktop: pet (`desktop/src/pet`).

pet-pet-built = built in
pet-pet-companion-pet = Pet
pet-pet-sprite-pet-s-sprite-sheet-could-not = This pet's sprite sheet could not be loaded
pet-pet-sprite-px-px = { $sheetWidth }px { $sheetHeight }px
pet-pet-sprite-px-px-2 = { $x }px { $y }px

## By hand — a sentence with a plural, a slot or several pieces; the generator keeps this section.
pet-pet-tile-showing = { $displayName }{ $flag ->
    [yes] {" "}— showing
   *[no] {""}
  }

## What the pet is doing, and a pet of your own (`pet/petModel.mjs`) — words moved out of the model.
pet-pet-yours = yours
pet-state = { $state ->
    [waiting] waiting on you
    [review] something to review
    [running] working
    [running-right] on the move
    [running-left] on the move
    [waving] saying hello
    [jumping] celebrating
    [failed] something failed
   *[idle] idle
  }
