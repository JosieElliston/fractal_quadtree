# presentation

## uncategorized

- computing each sample itself may seem embarrassingly parallel, but we might want to do things like blob detection for coloring, and we already have parallelism over sampling, so we don't have available threads to do parallelism inside sampling.
- note that the target hardware is "my m1 mac", not a cluster or smt, tho my solution should be fine for any common consumer hardware. note that i didn't end up using the gpu.

## slides

- slide: intro
    - the premiss is that i came up with a particular fractal to try to render, selected for being expensive, more than being mathematically interesting. (i enjoy performance engineering)
    - bc i have time, i'll define what it in some detail, tho this detail isn't very relevant to this class.
- slide: lecture structure
    - i'll give a brief overview of sampling,
    - then i'll talk about parallelism,
    - then i'll pad with more sampling
    - (sampling (TODO: right now at least) is really the hard part of this project)
- slide: the quadratic map
    - we're interested in iterating the function z^2 + c
    - iterating meaning z_{n+1} = z_n^2 + c
    - if we instead iterated c, we'll get something much less interesting
    - embed gif of varying c with lines between iterations (desmos)
- slide: mandelbrot set
    - note how some initial values for c stay bounded, while others diverge
    - a c value is inside the mandelbrot set if it stays bounded, and color it black
    - and otherwise color the c value based on how quickly it diverges
    - embed gif of varying c with lines between iterations (desmos) on top of the mandelbrot set
        - note that screen space is the c plane
- slide: z_0 != 0
    - you'll notice i never specified a base case for the function we're iterating
    - the previous slide used z_0 = 0, which is natural choice
    - but we can in fact pick other values
    - there are various things you might observe about these other mandelbrots, but what we care about is that some values of z_0 have area, while other don't
    - embed gif of the mandelbrot set with varying z_0 (desmos)
<!-- - slide: pt 2
    - desmos was bad, so i made my own renderer
    - but this was trivial and not that interesting
    - what i found interesting was to try render these quickly
    - but this turned out to be easy, this is nearly the canonical into to GPU project, and it's slow on the CPU either (TODO: find actual fps)
    - embed gif of the mandelbrot set with varying z_0 (fractal_explorer) -->
- slide: metabrot
    - well, lets draw this
    - instead of screen space being the c plane, screen space will be the z_0 plane
    - for each pixel (ie z_0 value), we'll color it black if the corresponding mandelbrot has area, otherwise we'll color it based on the maximum depth any point achieved before escaping
    - note you do a meta-julia set, you just get the mandelbrot set
    - this is incredibly slow, i can get a decent image in about a minute
    - but it would be nice to be able to pan/zoom in real time
    - the rest of this talk is on various speedups / the current architecture
