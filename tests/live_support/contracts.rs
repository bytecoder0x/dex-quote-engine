use alloy::sol;

sol! {
    #[sol(rpc)]
    interface IUniswapV3Pool {
        function slot0() external view returns (
            uint160 sqrtPriceX96, int24 tick, uint16 observationIndex,
            uint16 observationCardinality, uint16 observationCardinalityNext,
            uint8 feeProtocol, bool unlocked
        );
        function liquidity() external view returns (uint128 liquidity);
        function tickSpacing() external view returns (int24 tickSpacing);
        function fee() external view returns (uint24 fee);
        function token0() external view returns (address token0);
        function token1() external view returns (address token1);
        function tickBitmap(int16 wordPosition) external view returns (uint256 word);
        function ticks(int24 tick) external view returns (
            uint128 liquidityGross, int128 liquidityNet, uint256 feeGrowthOutside0X128,
            uint256 feeGrowthOutside1X128, int56 tickCumulativeOutside,
            uint160 secondsPerLiquidityOutsideX128, uint32 secondsOutside, bool initialized
        );
    }

    #[sol(rpc)]
    interface IQuoterV2 {
        struct QuoteExactInputSingleParams {
            address tokenIn;
            address tokenOut;
            uint256 amountIn;
            uint24 fee;
            uint160 sqrtPriceLimitX96;
        }
        function quoteExactInputSingle(QuoteExactInputSingleParams params) external returns (
            uint256 amountOut, uint160 sqrtPriceX96After, uint32 initializedTicksCrossed, uint256 gasEstimate
        );
    }

    #[sol(rpc)]
    interface IUniswapV2Pair {
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast);
        function token0() external view returns (address token0);
        function token1() external view returns (address token1);
    }

    #[sol(rpc)]
    interface IUniswapV2Router02 {
        function getAmountsOut(uint256 amountIn, address[] path) external view returns (uint256[] amounts);
    }
}
