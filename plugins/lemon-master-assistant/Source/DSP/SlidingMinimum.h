#pragma once
#include <algorithm>
#include <cstdint>
#include <vector>

namespace oxygen::dsp
{
    // Exact sliding minimum. Storage is allocated in prepare; push is allocation-free
    // and amortized O(1), independent of the lookahead duration and sample rate.
    class SlidingMinimum
    {
    public:
        void prepare(size_t windowSize)
        {
            entries.resize(std::max(size_t { 1 }, windowSize));
            reset();
        }

        void reset() { head = count = 0; position = 0; }

        float push(float value)
        {
            const auto capacity = entries.size();
            while (count > 0 && position - entries[head].position >= capacity)
            {
                head = (head + 1) % capacity;
                --count;
            }
            while (count > 0 && entries[(head + count - 1) % capacity].value >= value)
                --count;
            entries[(head + count) % capacity] = { value, position++ };
            ++count;
            return entries[head].value;
        }

    private:
        struct Entry { float value; uint64_t position; };
        std::vector<Entry> entries;
        size_t head = 0, count = 0;
        uint64_t position = 0;
    };
}
