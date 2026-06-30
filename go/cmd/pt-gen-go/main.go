// pt-gen-go generates typed Go code from md-tmpl .tmpl.md files.
//
// Usage:
//
//	pt-gen-go -input greeting.tmpl.md -output greeting_types.go -package mypackage
//
// Or with go:generate:
//
//	//go:generate pt-gen-go -input greeting.tmpl.md -output greeting_types.go -package mypackage
package main

import (
	"flag"
	"fmt"
	"log"
	"os"

	pt "github.com/domenukk/md-tmpl/go/md_tmpl"
)

func main() {
	inputPath := flag.String("input", "", "Path to the .tmpl.md template file (required)")
	outputPath := flag.String("output", "", "Path to write the generated Go file (required)")
	packageName := flag.String("package", "main", "Go package name for the generated file")
	paramsName := flag.String("params", "", "Name of the generated params struct (default: derived from filename)")
	noRender := flag.Bool("no-render", false, "Omit the Render helper method")

	flag.Usage = func() {
		fmt.Fprintf(os.Stderr, "Usage: pt-gen-go [flags]\n\nGenerates typed Go structs and enums from a .tmpl.md template file.\n\nFlags:\n")
		flag.PrintDefaults()
		fmt.Fprintf(os.Stderr, "\nExample:\n  pt-gen-go -input greeting.tmpl.md -output greeting_types.go -package mypackage\n")
	}
	flag.Parse()

	if *inputPath == "" || *outputPath == "" {
		flag.Usage()
		os.Exit(1)
	}

	var opts []pt.GenOption
	opts = append(opts, pt.WithPackageName(*packageName))
	if *paramsName != "" {
		opts = append(opts, pt.WithParamsName(*paramsName))
	}
	opts = append(opts, pt.WithRenderHelper(!*noRender))

	code, err := pt.GenerateTypesFromFile(*inputPath, opts...)
	if err != nil {
		log.Fatalf("Error generating types: %v", err)
	}

	if err := os.WriteFile(*outputPath, []byte(code), 0644); err != nil {
		log.Fatalf("Error writing output file: %v", err)
	}

	fmt.Fprintf(os.Stderr, "Generated %s from %s\n", *outputPath, *inputPath)
}
