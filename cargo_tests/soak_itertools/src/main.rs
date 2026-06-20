use itertools::Itertools;

fn main() {
    let nums: Vec<i32> = vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5];

    // .sorted() -> Vec<i32>
    let sorted: Vec<i32> = nums.iter().copied().sorted().collect();
    println!("sorted: {:?}", sorted);

    // .unique() -> Vec<i32> (preserves first-seen order)
    let unique: Vec<i32> = nums.iter().copied().unique().collect();
    println!("unique: {:?}", unique);

    // .chunks(n) -> chunk each group of 3 into a Vec, then sum each chunk
    let chunk_sums: Vec<i32> = nums
        .iter()
        .copied()
        .chunks(3)
        .into_iter()
        .map(|chunk| chunk.sum::<i32>())
        .collect();
    println!("chunk(3) sums: {:?}", chunk_sums);

    // .chunk_by (formerly group_by): group sorted nums by value, count run lengths
    let sorted_for_grouping: Vec<i32> = nums.iter().copied().sorted().collect();
    let group_counts: Vec<(i32, usize)> = sorted_for_grouping
        .iter()
        .copied()
        .chunk_by(|&x| x)
        .into_iter()
        .map(|(key, group)| (key, group.count()))
        .collect();
    println!("group counts: {:?}", group_counts);

    // .interleave() two sequences
    let a = vec![1, 3, 5];
    let b = vec![2, 4, 6];
    let interleaved: Vec<i32> = a.iter().copied().interleave(b.iter().copied()).collect();
    println!("interleaved: {:?}", interleaved);

    // .join(",") on &str values
    let words: Vec<&str> = vec!["alpha", "beta", "gamma", "delta"];
    let joined: String = words.iter().join(",");
    println!("joined: {}", joined);

    // sorted &str + unique
    let dup_words: Vec<&str> = vec!["pear", "apple", "pear", "fig", "apple", "kiwi"];
    let sorted_unique_words: Vec<&str> = dup_words.iter().copied().sorted().unique().collect();
    println!("sorted unique words: {:?}", sorted_unique_words);

    // min/max via itertools (returns Option, handled safely)
    match nums.iter().copied().minmax().into_option() {
        Some((mn, mx)) => println!("minmax: ({}, {})", mn, mx),
        None => println!("minmax: empty"),
    }

    println!("== soak_itertools done ==");
}
