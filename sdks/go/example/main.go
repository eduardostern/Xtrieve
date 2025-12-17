// Example usage of the Xtrieve Go client
package main

import (
	"encoding/binary"
	"fmt"
	"log"

	xtrieve ".."
)

func main() {
	fmt.Println("Xtrieve Go SDK Example")
	fmt.Println("======================")
	fmt.Println()

	// Connect to server
	fmt.Println("Connecting to 127.0.0.1:7419...")
	client, err := xtrieve.Connect("127.0.0.1", xtrieve.DefaultPort)
	if err != nil {
		log.Fatalf("Failed to connect: %v", err)
	}
	defer client.Close()
	fmt.Println("Connected!")
	fmt.Println()

	// Create a test file
	fmt.Println("Creating test file...")
	spec := &xtrieve.FileSpec{
		RecordLength: 100,
		PageSize:     4096,
		Keys: []xtrieve.KeySpec{
			{Position: 0, Length: 8, Flags: 0, Type: xtrieve.KeyTypeUnsignedBinary},
		},
	}

	resp, err := client.Create("go_example.dat", spec)
	if err != nil {
		log.Fatalf("Create failed: %v", err)
	}
	if resp.StatusCode != xtrieve.StatusSuccess && resp.StatusCode != 59 { // 59 = file exists
		fmt.Printf("Create status: %d\n", resp.StatusCode)
	} else {
		fmt.Println("File created (or exists)")
	}

	// Open the file
	fmt.Println()
	fmt.Println("Opening file...")
	resp, err = client.Open("go_example.dat", -1)
	if err != nil {
		log.Fatalf("Open failed: %v", err)
	}
	if resp.StatusCode != xtrieve.StatusSuccess {
		log.Fatalf("Open failed with status: %d", resp.StatusCode)
	}
	fmt.Println("File opened")
	posBlock := resp.PositionBlock

	// Insert some records
	fmt.Println()
	fmt.Println("Inserting records...")
	for i := 1; i <= 5; i++ {
		record := make([]byte, 100)

		// Write ID (8 bytes, little-endian)
		binary.LittleEndian.PutUint64(record[0:], uint64(i*1000))

		// Write name
		name := fmt.Sprintf("Record %d", i)
		copy(record[8:], name)

		resp, err = client.Insert(posBlock, record)
		if err != nil {
			log.Printf("Insert error: %v", err)
			continue
		}

		if resp.StatusCode == xtrieve.StatusSuccess {
			posBlock = resp.PositionBlock
			fmt.Printf("  Inserted record %d\n", i)
		} else if resp.StatusCode == xtrieve.StatusDuplicateKey {
			fmt.Printf("  Record %d already exists\n", i)
		} else {
			fmt.Printf("  Insert failed: %d\n", resp.StatusCode)
		}
	}

	// Read all records
	fmt.Println()
	fmt.Println("Reading all records:")
	count, err := client.ForEach(posBlock, 0, func(record, key []byte) error {
		id := binary.LittleEndian.Uint64(record[0:8])

		// Find end of name string
		nameEnd := 8
		for nameEnd < 40 && record[nameEnd] != 0 {
			nameEnd++
		}
		name := string(record[8:nameEnd])

		fmt.Printf("  ID: %d, Name: %s\n", id, name)
		return nil
	})

	if err != nil {
		log.Printf("Iteration error: %v", err)
	}
	fmt.Printf("  (End of file, %d records)\n", count)

	// Close file
	fmt.Println()
	fmt.Println("Closing file...")
	client.CloseFile(posBlock)

	fmt.Println()
	fmt.Println("Done!")
}
